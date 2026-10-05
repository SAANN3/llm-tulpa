//! The user's own words, kept in the prompt after a compaction fold.
//!
//! A summary can paraphrase a request; these are the messages themselves. Only the newest ones that
//! fit are kept, with no place reserved for the first: in a chat that runs for weeks the original
//! ask is the message most likely to be stale (the summary and the key facts carry the goal), and
//! what the user said last is what the next turn needs.

use super::{prompts, Agent};
use crate::services::error::ErrorService;

impl Agent {
    /// The block of the user's own folded-away messages as it goes into the system message (header
    /// included) for a chat folded up to `boundary_id`, or `None` when there are none. Built from the
    /// stored messages each time, so it only changes when a fold moves the boundary.
    pub(super) async fn pinned_section(&self, chat_id: i64, boundary_id: i64) -> Result<Option<String>, ErrorService> {
        let user_texts = self.chat_store.user_texts_up_to(chat_id, boundary_id).await?;
        Ok(with_header(&user_texts))
    }
}

/// How much of the user's folded-away words ride along verbatim after a fold (~1.5k tokens).
const PINNED_USER_CHARS: usize = 6_000;
/// One pinned message is cut here, so a pasted log can't use the whole allowance.
const PINNED_MESSAGE_CHARS: usize = 1_500;
/// How many ids of the left-out messages are named in the block.
const PINNED_LISTED_IDS: usize = 10;

/// The block with its header, or `None` when there are no messages.
fn with_header(messages: &[(i64, String)]) -> Option<String> {
    pin_user_messages(messages).map(|block| format!("{}{block}", prompts::PINNED_HEADER))
}

/// The user's folded-away messages as one block, in the order they were written: the newest ones that
/// still fit `PINNED_USER_CHARS`, and a line saying how many older ones are left out. Pure of anything
/// but the folded messages, so it only changes when a fold moves the boundary and the prompt prefix
/// stays cached in between.
///
/// Each entry is `(message id, text)`. A cut message, and the latest of the ones left out, say which
/// ids to read with `chat.get_messages`, so nothing the user wrote is out of reach.
fn pin_user_messages(messages: &[(i64, String)]) -> Option<String> {
    let cut = |id: i64, text: &str| -> String {
        let text = text.trim();
        match text.char_indices().nth(PINNED_MESSAGE_CHARS) {
            Some((end, _)) => format!("{}… [cut; the whole message: chat.get_messages id {id}]", &text[..end]),
            None => text.to_string(),
        }
    };
    let cut: Vec<(i64, String)> = messages
        .iter()
        .map(|(id, text)| (*id, cut(*id, text)))
        .filter(|(_, text)| !text.is_empty())
        .collect();
    let mut used = 0;
    let mut kept = 0;
    for (_, text) in cut.iter().rev() {
        used += text.chars().count();
        // The newest message always goes in: a cut one is at most PINNED_MESSAGE_CHARS long
        if used > PINNED_USER_CHARS && kept > 0 {
            break;
        }
        kept += 1;
    }
    if kept == 0 {
        return None;
    }
    let (skipped, shown) = cut.split_at(cut.len() - kept);
    let mut lines = Vec::new();
    if !skipped.is_empty() {
        let latest = &skipped[skipped.len().saturating_sub(PINNED_LISTED_IDS)..];
        let ids: Vec<String> = latest.iter().map(|(id, _)| id.to_string()).collect();
        lines.push(format!(
            "- [{} earlier message(s) not shown{} ids {}; chat.get_messages reads them, chat.list_messages lists them]",
            skipped.len(),
            if skipped.len() > latest.len() { ", the latest of them" } else { "," },
            ids.join(", "),
        ));
    }
    lines.extend(shown.iter().map(|(_, text)| format!("- {text}")));
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_to_pin_gives_no_block() {
        assert_eq!(pin_user_messages(&[]), None);
        assert_eq!(pin_user_messages(&[(1, "  ".to_string())]), None);
        assert_eq!(with_header(&[]), None);
    }

    #[test]
    fn a_few_short_messages_are_all_kept_in_order() {
        let few = vec![(1, "fix the bug".to_string()), (2, "also the test".to_string())];
        assert_eq!(pin_user_messages(&few).unwrap(), "- fix the bug\n- also the test");
        assert!(with_header(&few).unwrap().starts_with(prompts::PINNED_HEADER));
    }

    #[test]
    fn only_the_newest_fit_and_the_latest_skipped_ids_are_named() {
        let many: Vec<(i64, String)> = (0..40).map(|i| (100 + i, format!("{i}:{}", "x".repeat(498)))).collect();
        let block = pin_user_messages(&many).unwrap();
        // no place for the first message: the newest is there, the first is not
        assert!(block.contains("39:") && !block.contains("\n- 0:") && !block.starts_with("- 0:"), "{block}");
        // 6,000 characters of ~500 each: 11 kept, 29 left out, the latest ten of those named (119..=128)
        assert!(block.starts_with("- [29 earlier message(s) not shown, the latest of them ids 119, 120"), "{block}");
        assert!(block.contains("128; chat.get_messages"), "{block}");
        assert!(!block.contains("ids 100"), "{block}");
        assert!(block.chars().count() <= PINNED_USER_CHARS + 400);
        // Same input, same bytes: the prefix must not shift between turns.
        assert_eq!(block, pin_user_messages(&many).unwrap());
    }

    #[test]
    fn short_messages_keep_sixty_and_a_long_one_is_cut_with_its_id() {
        let short: Vec<(i64, String)> = (0..100).map(|i| (i, "y".repeat(100))).collect();
        let block = pin_user_messages(&short).unwrap();
        assert_eq!(block.lines().count(), 61, "60 messages and the line for the rest");
        let long = vec![(77, "z".repeat(5_000))];
        assert!(pin_user_messages(&long).unwrap().ends_with("[cut; the whole message: chat.get_messages id 77]"));
    }
}
