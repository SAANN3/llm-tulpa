//! Command-line handling shared by every tool that runs a shell command
//! (`os.execute_command`, `os.start_job`): what a command needs approval for, what may
//! never run, how a command line is prepared, and how its output is bounded. The
//! platform-specific parts of actually running one live in `services::process`.

use std::collections::HashMap;

use serde_json::Value;

use crate::tools::base::{ResolvedScope, ScopeGrant, SharedBucket, ToolPermission};

/// `description()` is compile-time-fixed text, but whether this container is actually
/// scoped away from the host or not depends on how the backend was launched, not on
/// anything decided at compile time — the same binary can run either way. Detecting it
/// once at runtime (Docker always creates `/.dockerenv`) and picking the matching text
/// keeps the model's own understanding of what this tool can reach accurate either way,
/// instead of baking in whichever answer happened to be true when this was written.
pub(super) fn running_in_docker() -> bool {
    std::path::Path::new("/.dockerenv").exists()
}

/// Same cap and truncation-marker convention as `storage.read_file`'s
/// `MAX_READ_CHARS` — this tool had none at all until a real incident: `grep -r`
/// across a directory containing a few minified/single-line files (VS Code
/// extension bundles) matched a handful of megabytes-long lines; `| head -20`
/// still let all of them through (it caps *lines*, not bytes), producing an
/// 8MB+ tool result that got stored as a real chat message. On the next turn,
/// sending that message as history blew ~30x past the model's context window;
/// Ollama silently truncated it down to fit (no error anywhere in the pipeline
/// — this shipped with zero logging for that path), and what survived excluded
/// nearly the entire actual conversation, so the model's very next reply looked
/// like it had genuinely forgotten everything — confirmed via Postgres directly
/// (`prompt_eval_count` cratered from ~43,000 to ~6,800 between consecutive
/// calls in the same chat, no compaction summary ever involved). Capping stdout
/// and stderr independently here is the actual fix — matches `storage.read_file`
/// so a single command can never again silently blow the whole context budget.
pub(super) const MAX_OUTPUT_CHARS: usize = 40_000;

pub(super) fn truncate_output(content: String) -> (String, bool) {
    let total_chars = content.chars().count();
    if total_chars <= MAX_OUTPUT_CHARS {
        return (content, false);
    }
    let cropped: String = content.chars().take(MAX_OUTPUT_CHARS).collect();
    (
        format!(
            "{cropped}\n\n[... output truncated: showing the first {MAX_OUTPUT_CHARS} of {total_chars} characters ...]"
        ),
        true,
    )
}

/// The last `MAX_OUTPUT_CHARS` characters of `content`, for output where the newest
/// part is the part that matters (a job's log) — the counterpart of `truncate_output`,
/// which keeps the first.
pub(super) fn tail_output(content: String) -> (String, bool) {
    let total_chars = content.chars().count();
    if total_chars <= MAX_OUTPUT_CHARS {
        return (content, false);
    }
    let cropped: String = content.chars().skip(total_chars - MAX_OUTPUT_CHARS).collect();
    (
        format!(
            "[... earlier output truncated: showing the last {MAX_OUTPUT_CHARS} of {total_chars} characters ...]\n\n{cropped}"
        ),
        true,
    )
}

fn parse_command_for_scope(cmd: &str) -> Option<String> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    for part in parts.iter() {
        if part.starts_with('/') || part.starts_with('.') || part.starts_with('~') {
            return Some(part.to_string());
        }
    }
    None
}

/// Words that start a statement without being the command themselves: the command is the word after
/// them (`then rm x`, `! grep -q`).
const SHELL_KEYWORDS_BEFORE_A_COMMAND: &[&str] = &["if", "then", "else", "elif", "do", "while", "until", "!", "{"];
/// Words that end a compound command, or start one whose next word isn't a command (`for x in ...`).
const SHELL_KEYWORDS_WITHOUT_A_COMMAND: &[&str] = &["fi", "done", "esac", "}", "for", "case", "select", "in", "function", "break", "continue", ":"];
/// Commands that run the command after them: both need approving, since approving `sudo` alone must not
/// cover whatever it is asked to run.
const COMMAND_WRAPPERS: &[&str] = &["sudo", "env", "nohup", "time", "nice", "exec", "command", "xargs", "timeout", "stdbuf"];

#[derive(Clone, Copy, PartialEq)]
enum ShellContext {
    Code,
    SingleQuoted,
    DoubleQuoted,
    /// `$( ... )`: code that ends at the matching `)`
    Substitution,
    /// `` ` ... ` ``: code that ends at the next backtick
    Backticks,
}

/// What `command_words` has read so far.
#[derive(Default)]
struct CommandScan {
    words: Vec<String>,
    word: String,
    /// The word being read starts a statement, so it is a command (or a keyword or assignment before one)
    at_statement: bool,
    /// The next word is a redirection's target (`> out.txt`), not a command
    redirect_target: bool,
}

impl CommandScan {
    fn end_word(&mut self) {
        if self.word.is_empty() {
            return;
        }
        let word = std::mem::take(&mut self.word);
        if std::mem::take(&mut self.redirect_target) {
            return;
        }
        // A redirection: `>`, `2>`, `>>` alone name their target in the next word; `>out.txt` holds it, and
        // `2>&1` points at another descriptor rather than a file
        let operator = word.trim_start_matches(|c: char| c.is_ascii_digit());
        if operator.starts_with(['<', '>']) {
            self.redirect_target = operator.trim_start_matches(['<', '>']).is_empty();
            return;
        }
        if !self.at_statement {
            return;
        }
        let is_assignment = word
            .split_once('=')
            .is_some_and(|(name, _)| !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        if is_assignment || SHELL_KEYWORDS_BEFORE_A_COMMAND.contains(&word.as_str()) {
            return;
        }
        if SHELL_KEYWORDS_WITHOUT_A_COMMAND.contains(&word.as_str()) {
            self.at_statement = false;
            return;
        }
        // A wrapper's own options and its `timeout 10`-style argument aren't the command it runs, and
        // `command -v x` only looks `x` up
        let after_wrapper = self.words.last().is_some_and(|last| COMMAND_WRAPPERS.contains(&last.as_str()));
        if after_wrapper && (word.starts_with('-') || is_duration(&word)) {
            if self.words.last().is_some_and(|last| last == "command") && (word == "-v" || word == "-V") {
                self.at_statement = false;
            }
            return;
        }
        self.at_statement = COMMAND_WRAPPERS.contains(&word.as_str());
        self.words.push(word);
    }

    fn new_statement(&mut self) {
        self.end_word();
        self.at_statement = true;
        self.redirect_target = false;
    }
}

/// `10`, `2.5`, `30s`, `5m`: the duration `timeout` takes before its command.
fn is_duration(word: &str) -> bool {
    word.trim_end_matches(['s', 'm', 'h', 'd']).parse::<f64>().is_ok()
}

/// Every command a command line would run, in order: the first word of each statement, wherever one
/// starts. That is after `;`, `&&`, `||`, `|`, `&`, a newline and `(`, inside `$( )` and backticks (also
/// inside double quotes, where the shell still runs them), and after keywords like `then` or `do`.
/// Leading `VAR=value` assignments are skipped, quotes come off a command word, a wrapper like `sudo`
/// counts along with the command it runs, and a heredoc's body, a comment and `$(( ))` arithmetic are
/// not commands.
///
/// Not a full shell parser: where it is unsure it finds more commands than will run (a word right after a
/// backtick inside an argument, say), never fewer, since every word it returns needs approving.
fn command_words(line: &str) -> Vec<String> {
    let mut scan = CommandScan { at_statement: true, ..Default::default() };
    let mut stack = vec![ShellContext::Code];
    let mut heredocs: Vec<(String, bool)> = Vec::new();
    let mut chars = line.chars().peekable();
    let mut previous = '\n';

    while let Some(c) = chars.next() {
        let context = *stack.last().unwrap_or(&ShellContext::Code);
        match context {
            ShellContext::SingleQuoted => {
                if c == '\'' {
                    stack.pop();
                } else {
                    scan.word.push(c);
                }
            }
            ShellContext::DoubleQuoted => match c {
                '"' => {
                    stack.pop();
                }
                '\\' => {
                    if let Some(next) = chars.next() {
                        scan.word.push(next);
                    }
                }
                '`' => {
                    scan.new_statement();
                    stack.push(ShellContext::Backticks);
                }
                '$' if chars.peek() == Some(&'(') => {
                    chars.next();
                    if chars.peek() == Some(&'(') {
                        skip_arithmetic(&mut chars);
                    } else {
                        scan.new_statement();
                        stack.push(ShellContext::Substitution);
                    }
                }
                _ => scan.word.push(c),
            },
            ShellContext::Code | ShellContext::Substitution | ShellContext::Backticks => match c {
                '\n' => {
                    scan.new_statement();
                    // A heredoc's body starts on the next line and runs to its delimiter: text, not commands
                    for (delimiter, strip_tabs) in std::mem::take(&mut heredocs) {
                        skip_heredoc(&mut chars, &delimiter, strip_tabs);
                    }
                }
                c if c.is_whitespace() => scan.end_word(),
                '#' if scan.word.is_empty() => {
                    while chars.peek().is_some_and(|&next| next != '\n') {
                        chars.next();
                    }
                }
                '\'' => stack.push(ShellContext::SingleQuoted),
                '"' => stack.push(ShellContext::DoubleQuoted),
                '\\' => {
                    if let Some(next) = chars.next() {
                        // A backslash before a newline only continues the line
                        if next != '\n' {
                            scan.word.push(next);
                        }
                    }
                }
                ';' | '|' | '(' => scan.new_statement(),
                // `2>&1`, `>&2` and `&>` are redirections, not a background `&`
                '&' if previous == '>' || previous == '<' || chars.peek() == Some(&'>') => scan.word.push(c),
                '&' => scan.new_statement(),
                ')' => {
                    scan.end_word();
                    if context == ShellContext::Substitution {
                        stack.pop();
                    }
                    scan.at_statement = false;
                }
                '`' => {
                    if context == ShellContext::Backticks {
                        scan.end_word();
                        stack.pop();
                        scan.at_statement = false;
                    } else {
                        scan.new_statement();
                        stack.push(ShellContext::Backticks);
                    }
                }
                '$' if chars.peek() == Some(&'(') => {
                    chars.next();
                    if chars.peek() == Some(&'(') {
                        skip_arithmetic(&mut chars);
                    } else {
                        scan.new_statement();
                        stack.push(ShellContext::Substitution);
                    }
                }
                '<' if chars.peek() == Some(&'<') => {
                    chars.next();
                    if chars.peek() == Some(&'<') {
                        // `<<<` takes a string, not a heredoc
                        chars.next();
                        scan.end_word();
                        scan.redirect_target = true;
                    } else {
                        let strip_tabs = chars.next_if_eq(&'-').is_some();
                        while chars.next_if(|next| *next == ' ' || *next == '\t').is_some() {}
                        let mut delimiter = String::new();
                        while let Some(next) = chars.next_if(|next| !next.is_whitespace() && !matches!(next, ';' | '|' | '&' | ')')) {
                            if !matches!(next, '\'' | '"' | '\\') {
                                delimiter.push(next);
                            }
                        }
                        scan.end_word();
                        if !delimiter.is_empty() {
                            heredocs.push((delimiter, strip_tabs));
                        }
                    }
                }
                '>' | '<' => {
                    scan.end_word();
                    scan.word.push(c);
                }
                _ => scan.word.push(c),
            },
        }
        previous = c;
    }
    scan.end_word();
    scan.words
}

/// Moves past a heredoc's body, up to and including the line that holds only its delimiter (with `<<-`,
/// after leading tabs).
fn skip_heredoc(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, delimiter: &str, strip_tabs: bool) {
    loop {
        let mut line = String::new();
        let mut ended = false;
        for c in chars.by_ref() {
            if c == '\n' {
                ended = true;
                break;
            }
            line.push(c);
        }
        let text = if strip_tabs { line.trim_start_matches('\t') } else { line.as_str() };
        if text.trim_end_matches('\r') == delimiter || !ended {
            return;
        }
    }
}

/// Moves past `$(( ... ))` once its `$((` is read: the parentheses inside it are arithmetic, not commands.
fn skip_arithmetic(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    let mut depth = 1;
    for c in chars.by_ref() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    // The second `)` of the closing `))`
                    break;
                }
            }
            _ => {}
        }
    }
}

/// Shell preamble (see `call_untyped`) that makes `apt-get`/`apt`/`dpkg` resolve to a
/// sudo'd invocation of themselves wherever they're actually called in the command
/// that follows — not just when one happens to be the very first word. Aliases (not
/// shell functions — this container's `/bin/sh` is dash, whose POSIX-strict function-
/// name grammar rejects the hyphen in `apt-get`, making `apt-get() { ...; }` a flat
/// syntax error there, breaking the *entire* script it's prepended to; `alias`, unlike
/// a function name, isn't restricted to identifier syntax) take part in normal command
/// lookup the same as a real binary would, so this transparently covers every position
/// a naive "is the command's first word one of these" check would miss: chained with
/// `&&`/`;`, inside a loop, after a pipe, however many times — confirmed live in dash
/// across all of those. (An earlier version spliced `sudo -n` into just the command's
/// leading word instead — that left a case like `apt-get update && apt-get install x`
/// with only the first `apt-get` elevated, so the second still hit dpkg's lock file
/// with a real "Permission denied.") The container's own non-root user is granted a
/// scoped, passwordless `sudo` covering exactly these three binaries (see the
/// Dockerfile's `pkg-mgmt` sudoers rule) — this exists so the model doesn't need to
/// already know this particular container requires `sudo` just to install a package.
/// Doesn't touch approval: `command_words`/`is_dangerous` still see `"apt-get"` etc. as
/// the command word being approved, same as any other — this only changes how it's
/// actually invoked once permitted. Defining these unconditionally rather than only
/// when the command appears to need them is harmless (an unused alias costs nothing)
/// and is exactly what avoids re-implementing shell parsing to detect every position
/// one might be called from.
const PACKAGE_MANAGER_SUDO_PREAMBLE: &str = "alias apt-get='sudo -n /usr/bin/apt-get'; \
     alias apt='sudo -n /usr/bin/apt'; \
     alias dpkg='sudo -n /usr/bin/dpkg';\n";

/// The command line to actually hand to the shell: `command` as the model wrote it,
/// with the package-manager preamble in front when running under Docker (see
/// `PACKAGE_MANAGER_SUDO_PREAMBLE`), and unchanged everywhere else.
pub(super) fn effective_command(command: &str) -> String {
    if running_in_docker() {
        format!("{PACKAGE_MANAGER_SUDO_PREAMBLE}{command}")
    } else {
        command.to_string()
    }
}

/// Whether `command` may run, given which command words have already been approved (`approved_scope`: the
/// `SharedBucket::ShellCommands` grant, if any). Every command the line would run has to be approved, not
/// only its first one: chaining (`;`, `&&`, `|`, `$( )` ...) is how one approved word would otherwise carry
/// any other command through. The blocklist applies whatever is approved.
pub(crate) fn check_command_permission(command: &str, workdir: Option<&str>, approved_scope: Option<&Value>) -> ToolPermission {
    let commands = command_words(command);
    let cmd_lower = command.to_lowercase();
    let has_destructive_command = commands
        .iter()
        .map(|word| word.to_lowercase())
        .any(|word| word == "dd" || word == "mkfs" || word.starts_with("mkfs."));
    if cmd_lower.contains("rm -rf /") || cmd_lower.contains("> /dev/sd") || has_destructive_command {
        return ToolPermission::Denied {
            reason: "Blocked obviously destructive command pattern".to_string(),
            escalation: None,
        };
    }

    if commands.is_empty() {
        return ToolPermission::Denied {
            reason: "couldn't find a command to run in that line".to_string(),
            escalation: None,
        };
    }

    let approved = approved_scope
        .and_then(|scope| scope.get(SharedBucket::ShellCommands.json_key()))
        .and_then(|commands| commands.as_object());
    let mut missing: Vec<&str> = Vec::new();
    for word in &commands {
        if !approved.is_some_and(|approved| approved.contains_key(word)) && !missing.contains(&word.as_str()) {
            missing.push(word);
        }
    }
    if missing.is_empty() {
        return ToolPermission::Allowed;
    }

    let path = workdir.map(str::to_string).or_else(|| parse_command_for_scope(command));
    let reason = match &path {
        Some(path) => format!("Execute command requires approval (path context: {path})"),
        None => "Execute command requires approval".to_string(),
    };
    let named = missing.iter().map(|word| format!("`{word}`")).collect::<Vec<_>>().join(", ");
    let grant: serde_json::Map<String, Value> = missing.iter().map(|word| (word.to_string(), Value::Bool(true))).collect();

    ToolPermission::Denied {
        reason,
        escalation: Some(ScopeGrant {
            scope: ResolvedScope {
                own: None,
                shared: HashMap::from([(
                    SharedBucket::ShellCommands,
                    serde_json::json!({ SharedBucket::ShellCommands.json_key(): grant }),
                )]),
            },
            ui_message: if missing.len() == 1 {
                format!(
                    "Allow running {named} (with any arguments) for the rest of this chat? Only this command — a line \
                     that also runs another one still asks for that one."
                )
            } else {
                format!(
                    "This line runs {named}. Allow these commands (with any arguments) for the rest of this chat? \
                     Only these — a line that also runs another one still asks for that one."
                )
            },
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        command_words(line)
    }

    fn approved(commands: &[&str]) -> Value {
        let map: serde_json::Map<String, Value> = commands.iter().map(|c| (c.to_string(), Value::Bool(true))).collect();
        serde_json::json!({ SharedBucket::ShellCommands.json_key(): map })
    }

    /// The commands the escalation asks for, or `None` when the line is allowed as it stands
    fn asks_for(line: &str, granted: &[&str]) -> Option<Vec<String>> {
        match check_command_permission(line, None, Some(&approved(granted))) {
            ToolPermission::Allowed => None,
            ToolPermission::Denied { escalation: Some(grant), .. } => {
                let scope = &grant.scope.shared[&SharedBucket::ShellCommands];
                Some(scope[SharedBucket::ShellCommands.json_key()].as_object().unwrap().keys().cloned().collect())
            }
            ToolPermission::Denied { escalation: None, reason } => panic!("refused outright: {reason}"),
        }
    }

    #[test]
    fn every_statement_of_a_line_is_a_command() {
        assert_eq!(words("ls -la"), ["ls"]);
        assert_eq!(words("ls; rm -rf ~/x"), ["ls", "rm"]);
        assert_eq!(words("cd /src && cargo build 2>&1 | tail -5"), ["cd", "cargo", "tail"]);
        assert_eq!(words("make || echo failed &"), ["make", "echo"]);
        assert_eq!(words("a\nb\n\nc"), ["a", "b", "c"]);
        assert_eq!(words("(cd x; ls) > out.txt"), ["cd", "ls"]);
        assert_eq!(words("FOO=1 BAR=x make test"), ["make"]);
    }

    #[test]
    fn substitutions_run_commands_even_inside_double_quotes() {
        assert_eq!(words("echo $(whoami)"), ["echo", "whoami"]);
        assert_eq!(words("echo \"user: $(id -u)\""), ["echo", "id"]);
        assert_eq!(words("echo `date` > now.txt"), ["echo", "date"]);
        assert_eq!(words("echo \"`uname -a`\""), ["echo", "uname"]);
        // Arithmetic isn't a command
        assert_eq!(words("echo $((1 + 2)) $(( (3) ))"), ["echo"]);
    }

    #[test]
    fn quoted_text_comments_and_heredocs_are_not_commands() {
        assert_eq!(words("git commit -m 'fix; then rm && test'"), ["git"]);
        assert_eq!(words("echo \"a; b | c\""), ["echo"]);
        assert_eq!(words("ls # list it; then delete"), ["ls"]);
        assert_eq!(
            words("cat > app.py <<'EOF'\nimport os; print(os.getcwd())\nrm = 1\nEOF\npython3 app.py"),
            ["cat", "python3"]
        );
        assert_eq!(words("cat <<-END | wc -l\n\tone; two\n\tEND\necho done"), ["cat", "wc", "echo"]);
        assert_eq!(words("grep x <<< \"$text\""), ["grep"]);
        assert_eq!(words("python3 - <<EOF\nprint(1)\nEOF"), ["python3"]);
    }

    #[test]
    fn keywords_and_wrappers() {
        assert_eq!(words("if [ -f x ]; then rm x; else touch x; fi"), ["[", "rm", "touch"]);
        assert_eq!(words("for f in *.txt; do wc -l \"$f\"; done"), ["wc"]);
        assert_eq!(words("while read l; do echo $l; done < in.txt"), ["read", "echo"]);
        assert_eq!(words("sudo -n apt-get install -y jq"), ["sudo", "apt-get"]);
        assert_eq!(words("timeout 30s npm test"), ["timeout", "npm"]);
        assert_eq!(words("find . -name '*.log' | xargs -0 rm"), ["find", "xargs", "rm"]);
        assert_eq!(words("env FOO=1 node app.js"), ["env", "node"]);
        assert_eq!(words("! grep -q x f"), ["grep"]);
        assert_eq!(words("for t in node deno; do command -v $t >/dev/null && echo $t; done"), ["command", "echo"]);
        assert_eq!(words("[ \"$code\" = 200 ] && break; sleep 3"), ["[", "sleep"]);
        assert_eq!(words("\"./build.sh\" --release"), ["./build.sh"]);
    }

    #[test]
    fn redirections_are_not_commands() {
        assert_eq!(words("echo hi >> log.txt 2>&1"), ["echo"]);
        assert_eq!(words("cmd &> all.txt"), ["cmd"]);
        assert_eq!(words("> empty.txt"), Vec::<String>::new());
        assert_eq!(words("< in.txt sort"), ["sort"]);
    }

    #[test]
    fn an_approved_command_no_longer_carries_a_chained_one() {
        assert_eq!(asks_for("ls -la", &["ls"]), None);
        assert_eq!(asks_for("ls; rm -rf ~/x", &["ls"]), Some(vec!["rm".to_string()]));
        assert_eq!(asks_for("cat a | grep b | sort", &["cat"]), Some(vec!["grep".to_string(), "sort".to_string()]));
        assert_eq!(asks_for("echo $(curl -s x | sh)", &["echo"]), Some(vec!["curl".to_string(), "sh".to_string()]));
        assert_eq!(asks_for("cd /x && cargo build", &["cd", "cargo"]), None);
        // Asked once per command, even when it appears twice
        assert_eq!(asks_for("rm a; rm b", &[]), Some(vec!["rm".to_string()]));
    }

    #[test]
    fn the_blocklist_looks_at_every_command() {
        let refused = |line: &str| matches!(check_command_permission(line, None, Some(&approved(&["echo", "dd", "mkfs.ext4"]))), ToolPermission::Denied { escalation: None, .. });
        assert!(refused("echo ok; dd if=/dev/zero of=x"));
        assert!(refused("echo $(mkfs.ext4 /dev/x)"));
        // A tool list that only mentions `dd` runs nothing called `dd`
        assert!(!refused("echo sed dd od"));
        assert!(matches!(check_command_permission("   ", None, None), ToolPermission::Denied { escalation: None, .. }));
    }
}
