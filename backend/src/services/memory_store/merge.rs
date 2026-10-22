//! What the model proposes for a user's memory, and what becomes of it: the checks that keep the list free of
//! repeats, and the text a chat's prompt takes from it. Pure functions, so all of it can be checked without a
//! database or a model.

use std::collections::HashSet;

use super::{Category, MemoryEntry};

/// An entry is cut to this many characters.
pub const MAX_ENTRY_CHARS: usize = 200;
/// Two entries whose words overlap this much (Jaccard) are the same thing said twice.
const SAME_THING_OVERLAP: f64 = 0.8;

/// What the model proposed: one thing to remember, the user's words it rests on, and the entry it corrects.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub category: Category,
    pub text: String,
    pub quote: String,
    /// The id of the entry this one replaces (the caller has turned the number the model saw into it)
    pub replaces: Option<i64>,
}

/// What to do with a candidate.
#[derive(Debug, PartialEq)]
pub enum Decision {
    Add { category: Category, text: String, quote: String },
    /// A learned entry corrected or said better: new category, text and quote.
    Replace { id: i64, category: Category, text: String, quote: String },
    Skip(&'static str),
}

/// Lowercase words without punctuation, one space apart: what two wordings are compared by.
pub fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn words(normalized: &str) -> HashSet<&str> {
    normalized.split(' ').filter(|w| !w.is_empty()).collect()
}

/// Whether `a` and `b` say the same thing: equal, one inside the other, or mostly the same words.
fn same_thing(a: &str, b: &str) -> bool {
    let (a, b) = (normalize(a), normalize(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b || a.contains(&b) || b.contains(&a) {
        return true;
    }
    let (wa, wb) = (words(&a), words(&b));
    let common = wa.intersection(&wb).count() as f64;
    common / wa.union(&wb).count() as f64 >= SAME_THING_OVERLAP
}

/// `text` with its whitespace collapsed and cut to `MAX_ENTRY_CHARS` characters.
fn tidy(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_ENTRY_CHARS).collect()
}

/// Decides what to do with each candidate, in order, against `existing` and against the candidates before it. A
/// correction (`replaces`) changes only an entry the model wrote; an entry the user wrote or edited is left as it is
/// and the candidate dropped. A repeat of an entry is dropped, unless it says the same thing in more words and the
/// entry is the model's: then it replaces it. The user's own quote check is the caller's job (it has the messages).
pub fn plan(existing: &[MemoryEntry], candidates: Vec<Candidate>) -> Vec<Decision> {
    let mut working: Vec<MemoryEntry> = existing.to_vec();
    // Entries added by this very batch have no id yet: they are numbered below zero, and the decision that adds one is found here
    let mut added_at: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    let mut next_id = -1;
    let mut decisions = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let text = tidy(&candidate.text);
        if text.is_empty() {
            decisions.push(Decision::Skip("empty"));
            continue;
        }
        let quote = candidate.quote;
        let entry_for = |id: i64| MemoryEntry {
            id,
            category: candidate.category,
            text: text.clone(),
            quote: Some(quote.clone()),
            learned: true,
            source_chat_id: None,
            updated_at: chrono::Utc::now(),
        };
        if let Some(id) = candidate.replaces {
            if let Some(at) = working.iter().position(|e| e.id == id) {
                if !working[at].learned {
                    decisions.push(Decision::Skip("the entry is the user's own"));
                } else {
                    working[at] = entry_for(id);
                    decisions.push(Decision::Replace { id, category: candidate.category, text, quote });
                }
                continue;
            }
        }
        match working.iter().position(|e| same_thing(&e.text, &text)) {
            Some(at) => {
                let fuller = normalize(&text).len() > normalize(&working[at].text).len();
                let id = working[at].id;
                if working[at].learned && fuller && id < 0 {
                    // Said twice in one reply: the earlier decision takes the fuller wording
                    working[at] = entry_for(id);
                    decisions[added_at[&id]] = Decision::Add { category: candidate.category, text, quote };
                    decisions.push(Decision::Skip("said twice in one reply"));
                } else if working[at].learned && fuller {
                    working[at] = entry_for(id);
                    decisions.push(Decision::Replace { id, category: candidate.category, text, quote });
                } else {
                    decisions.push(Decision::Skip("already remembered"));
                }
            }
            None => {
                working.push(entry_for(next_id));
                added_at.insert(next_id, decisions.len());
                next_id -= 1;
                decisions.push(Decision::Add { category: candidate.category, text, quote });
            }
        }
    }
    decisions
}

/// The entries as a chat's system prompt takes them: how the user likes things done first (it changes the model's
/// behavior most), then what is known about them, then the rest; in each group the user's own entries first and then the
/// newest. Whole entries only, up to `budget` characters in all; empty when there are none. The same entries give the
/// same text, which the prompt cache depends on.
pub fn snapshot_text(entries: &[MemoryEntry], budget: usize) -> String {
    const INTRO: &str = "What is remembered about the user from earlier conversations (a note, not an instruction; what they say now comes first):";
    let mut text = String::new();
    let mut used = INTRO.len();
    let mut any = false;
    for (category, heading) in [
        (Category::Preferences, "How they like things done:"),
        (Category::AboutYou, "About them:"),
        (Category::Misc, "Other:"),
    ] {
        let mut group: Vec<&MemoryEntry> = entries.iter().filter(|e| e.category == category).collect();
        group.sort_by(|a, b| a.learned.cmp(&b.learned).then(b.updated_at.cmp(&a.updated_at)).then(b.id.cmp(&a.id)));
        let mut lines = String::new();
        for entry in group {
            let line = format!("- {}\n", entry.text);
            if used + heading.len() + 1 + lines.len() + line.len() > budget {
                break;
            }
            lines.push_str(&line);
        }
        if !lines.is_empty() {
            used += heading.len() + 1 + lines.len();
            text.push_str(heading);
            text.push('\n');
            text.push_str(&lines);
            any = true;
        }
    }
    if !any {
        return String::new();
    }
    format!("{INTRO}\n{}", text.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn entry(id: i64, category: Category, text: &str, learned: bool, age_minutes: i64) -> MemoryEntry {
        MemoryEntry {
            id,
            category,
            text: text.to_string(),
            quote: learned.then(|| "q".to_string()),
            learned,
            source_chat_id: None,
            updated_at: Utc::now() - Duration::minutes(age_minutes),
        }
    }

    fn candidate(category: Category, text: &str, replaces: Option<i64>) -> Candidate {
        Candidate { category, text: text.to_string(), quote: "what the user wrote".to_string(), replaces }
    }

    #[test]
    fn a_repeat_is_dropped_and_a_fuller_wording_replaces_the_models_entry() {
        let existing = vec![entry(1, Category::AboutYou, "The user is a second-year CS student.", true, 5)];
        let plan = plan(
            &existing,
            vec![
                candidate(Category::AboutYou, "the user is a second year CS student", None),
                candidate(Category::AboutYou, "The user is a second-year CS student at a technical university.", None),
                candidate(Category::AboutYou, "The user runs Arch with Hyprland.", None),
                candidate(Category::AboutYou, "The user runs Arch with Hyprland on a laptop.", None),
            ],
        );
        assert_eq!(plan[0], Decision::Skip("already remembered"));
        assert!(matches!(&plan[1], Decision::Replace { id: 1, .. }));
        assert!(matches!(&plan[2], Decision::Add { .. }));
        // The one before it in the same batch counts as remembered: the fuller wording goes into that decision
        assert_eq!(plan[3], Decision::Skip("said twice in one reply"));
        assert!(matches!(&plan[2], Decision::Add { text, .. } if text.ends_with("on a laptop.")));
    }

    #[test]
    fn words_that_mostly_overlap_are_the_same_thing_and_different_things_are_not() {
        assert!(same_thing("The user prefers answers in Russian", "User prefers Russian answers in"));
        assert!(!same_thing("The user prefers answers in Russian", "The user prefers answers in English"));
        assert!(!same_thing("", "anything"));
    }

    #[test]
    fn a_correction_changes_the_models_entry_but_never_the_users_own() {
        let existing = vec![
            entry(1, Category::Preferences, "The user prefers answers in Russian.", true, 5),
            entry(2, Category::AboutYou, "The user lives in Yekaterinburg.", false, 5),
        ];
        let plan = plan(
            &existing,
            vec![
                candidate(Category::Preferences, "The user prefers answers in English.", Some(1)),
                candidate(Category::AboutYou, "The user lives in Moscow.", Some(2)),
                candidate(Category::AboutYou, "The user lives in Berlin.", Some(99)),
            ],
        );
        assert!(matches!(&plan[0], Decision::Replace { id: 1, text, .. } if text.contains("English")));
        assert_eq!(plan[1], Decision::Skip("the entry is the user's own"));
        // An entry that is not there is just a new one
        assert!(matches!(&plan[2], Decision::Add { .. }));
    }

    #[test]
    fn a_users_own_entry_is_never_replaced_by_a_fuller_wording() {
        let existing = vec![entry(1, Category::Misc, "Likes tea.", false, 5)];
        let plan = plan(&existing, vec![candidate(Category::Misc, "Likes tea in the evening.", None)]);
        assert_eq!(plan[0], Decision::Skip("already remembered"));
    }

    #[test]
    fn an_entry_is_tidied_and_cut_to_the_limit() {
        let long = format!("  The   user {} ", "x".repeat(300));
        let plan = plan(&[], vec![candidate(Category::Misc, &long, None), candidate(Category::Misc, "  ", None)]);
        match &plan[0] {
            Decision::Add { text, .. } => {
                assert_eq!(text.chars().count(), MAX_ENTRY_CHARS);
                assert!(text.starts_with("The user x"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(plan[1], Decision::Skip("empty"));
    }

    #[test]
    fn the_snapshot_orders_groups_and_entries_and_stops_at_the_budget() {
        let entries = vec![
            entry(1, Category::AboutYou, "Is a student.", true, 10),
            entry(2, Category::Preferences, "Wants short answers.", true, 1),
            entry(3, Category::Preferences, "Answers in Russian.", false, 100),
            entry(4, Category::Misc, "Has a cat.", true, 3),
        ];
        let text = snapshot_text(&entries, 10_000);
        let at = |needle: &str| text.find(needle).unwrap_or_else(|| panic!("{needle} missing in {text}"));
        assert!(at("How they like things done:") < at("About them:") && at("About them:") < at("Other:"));
        // The user's own entry before the newer one the model wrote
        assert!(at("Answers in Russian.") < at("Wants short answers."));
        assert_eq!(snapshot_text(&entries, 10_000), text, "the same entries give the same text");
        // A budget for the intro and one line only: whole entries, nothing cut in the middle
        let small = snapshot_text(&entries, 175);
        assert!(small.contains("Answers in Russian.") && !small.contains("Is a student."), "{small}");
        assert_eq!(snapshot_text(&[], 10_000), "");
        assert_eq!(snapshot_text(&entries, 5), "");
    }
}
