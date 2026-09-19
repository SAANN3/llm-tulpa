//! Everything platform-specific about running a shell command line, in one place —
//! `services::job_store` and `tools::os::execute_command` both build their processes
//! through this, so supporting another platform means touching exactly this file. Only
//! the `unix` and `windows` branches are written; every other target is treated like
//! Unix.

use std::path::PathBuf;

/// A `Command` that runs `command_line` through the platform's shell: `sh -c` on
/// everything but Windows, `cmd /C` there. `workdir`, when given, is where it starts.
pub fn shell_command(command_line: &str, workdir: Option<PathBuf>) -> tokio::process::Command {
    #[cfg(windows)]
    let mut command = {
        let mut command = tokio::process::Command::new("cmd");
        command.arg("/C").arg(command_line);
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut command = tokio::process::Command::new("sh");
        command.arg("-c").arg(command_line);
        command
    };

    if let Some(workdir) = workdir {
        command.current_dir(workdir);
    }
    command
}

/// Puts the command in a process group/tree of its own, so [`kill_process_tree`] can
/// later take out everything it started without touching anything else — and so a
/// terminal-style signal aimed at the backend's own group never reaches it.
pub fn detach(command: &mut tokio::process::Command) {
    #[cfg(unix)]
    command.process_group(0);

    #[cfg(windows)]
    {
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }

    #[cfg(not(any(unix, windows)))]
    let _ = command;
}

/// Kills `pid` and everything it started. On Unix that's the whole process group
/// (`detach` made `pid` its leader), which also reaches anything the tree re-parented
/// away from it; elsewhere it walks parent links to find the descendants. Best-effort
/// — a process that's already gone is not an error.
pub async fn kill_process_tree(pid: u32) {
    #[cfg(unix)]
    {
        // `kill -s KILL -- -PID` (not `-9`, and not `-9 --`): the only spelling that
        // both dash and bash accept for a process-group target.
        let _ = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(format!("kill -s KILL -- -{pid}"))
            .status()
            .await;
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::task::spawn_blocking(move || kill_descendants(pid)).await;
    }
}

/// Kills `root` and every process descending from it, deepest first so a parent can't
/// respawn a child that was just killed.
#[cfg_attr(unix, allow(dead_code))]
fn kill_descendants(root: u32) {
    use sysinfo::{Pid, ProcessesToUpdate, System};

    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    let mut tree = vec![Pid::from_u32(root)];
    let mut index = 0;
    while index < tree.len() {
        let parent = tree[index];
        for (pid, process) in system.processes() {
            if process.parent() == Some(parent) && !tree.contains(pid) {
                tree.push(*pid);
            }
        }
        index += 1;
    }

    for pid in tree.into_iter().rev() {
        if let Some(process) = system.process(pid) {
            process.kill();
        }
    }
}
