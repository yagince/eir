use std::collections::{HashMap, HashSet};

use crate::github::{NotificationItem, WatchedItem};

/// Key used to join a watched item (from the search query) with a
/// notification thread (from /notifications). Matches the JS `itemKey` shape
/// — `repo:kind:number`.
pub fn item_key(repo: &str, kind: &str, number: u64) -> String {
    format!("{repo}:{kind}:{number}")
}

/// A change worth notifying the user about. `reason` is already the
/// human-readable phrase the notification title should carry.
#[derive(Clone)]
pub struct ItemChange {
    pub item: WatchedItem,
    pub reason: String,
}

/// Fresh-notification diff: a thread is "fresh" when we've never seen it
/// before, or its `updated_at` has advanced (comments bump updated_at on the
/// same thread_id).
pub fn fresh_notifications(
    prev_threads: &HashMap<u64, String>,
    curr: &[NotificationItem],
) -> Vec<NotificationItem> {
    curr.iter()
        .filter(|n| {
            prev_threads
                .get(&n.thread_id)
                .is_none_or(|prev_updated| prev_updated != &n.updated_at)
        })
        .cloned()
        .collect()
}

/// Compare a previous snapshot of the item list against a fresh fetch and
/// produce one `ItemChange` per item that meaningfully moved (new entries or
/// anything `describe_item_change` cares about). Items whose `(repo, kind,
/// number)` is already covered by a GitHub notification thread in
/// `notified_keys` are skipped — the /notifications diff gets first dibs so
/// we don't fire two desktop notifications for the same underlying event.
pub fn compute_item_changes(
    prev_items: &HashMap<u64, WatchedItem>,
    curr_items: &[WatchedItem],
    notified_keys: &HashSet<String>,
) -> Vec<ItemChange> {
    let mut out = Vec::new();
    for item in curr_items {
        if notified_keys.contains(&item_key(&item.repo, item.kind, item.number)) {
            continue;
        }
        match prev_items.get(&item.id) {
            None => out.push(ItemChange {
                item: item.clone(),
                reason: "New in list".into(),
            }),
            Some(prev) => {
                if let Some(reason) = describe_item_change(prev, item) {
                    out.push(ItemChange {
                        item: item.clone(),
                        reason,
                    });
                }
            }
        }
    }
    out
}

/// Describe what meaningfully changed between two snapshots of the same item,
/// as a short phrase suitable for a desktop-notification title. Returns None
/// when nothing interesting changed.
///
/// Order matters — we return the first signal we find, roughly ranked by how
/// actionable the change is: CI failures are louder than a label tweak.
pub fn describe_item_change(prev: &WatchedItem, curr: &WatchedItem) -> Option<String> {
    // PR state transitions (merged / closed / reopened) are the biggest news.
    if prev.state != curr.state {
        return Some(format!("Now {}", curr.state));
    }

    // Draft ⇄ ready is a reviewer-relevant flip.
    if prev.is_draft != curr.is_draft {
        return Some(
            if curr.is_draft {
                "Marked as draft"
            } else {
                "Ready for review"
            }
            .to_string(),
        );
    }

    // CI state change (includes success after a red build — also worth hearing).
    if prev.ci_status != curr.ci_status {
        return Some(format!("CI {}", curr.ci_status.unwrap_or("unknown")));
    }

    // Review-state change for any reviewer. Prefer the first non-matching one
    // to keep the output short; the full state lives in the app itself.
    let prev_by_login: HashMap<&str, &str> = prev
        .reviewers
        .iter()
        .map(|r| (r.login.as_str(), r.state))
        .collect();
    for r in &curr.reviewers {
        if prev_by_login.get(r.login.as_str()).copied() != Some(r.state) {
            return Some(format!("{} {}", r.login, r.state.replace('_', " ")));
        }
    }

    // Comment count went up — someone commented.
    if curr.comments > prev.comments {
        let delta = curr.comments - prev.comments;
        return Some(format!(
            "+{delta} comment{}",
            if delta == 1 { "" } else { "s" }
        ));
    }

    // Nothing above matched but updated_at moved — some edit happened we
    // don't model (labels, assignees, etc). Worth a quiet catch-all so the
    // user knows something changed upstream.
    if prev.updated_at != curr.updated_at {
        return Some("Updated".into());
    }

    None
}

/// Items that existed in the previous snapshot but have dropped out of the
/// current fetch. The server query is `is:open`, so in practice this means
/// the item was closed / merged / archived. Items already covered by a fresh
/// notification are filtered out to avoid a double-ping.
pub fn removed_items(
    prev_items: &HashMap<u64, WatchedItem>,
    curr_items: &[WatchedItem],
    notified_keys: &HashSet<String>,
) -> Vec<WatchedItem> {
    let curr_ids: HashSet<u64> = curr_items.iter().map(|i| i.id).collect();
    prev_items
        .values()
        .filter(|prev| {
            !curr_ids.contains(&prev.id)
                && !notified_keys.contains(&item_key(&prev.repo, prev.kind, prev.number))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::{Reviewer, WatchedItem};

    fn make_item(id: u64, repo: &str, kind: &'static str, number: u64) -> WatchedItem {
        WatchedItem {
            id,
            kind,
            title: "t".into(),
            number,
            repo: repo.into(),
            url: "".into(),
            author: "".into(),
            author_avatar: "".into(),
            comments: 0,
            updated_at: "2026-01-01T00:00:00Z".into(),
            state: "open",
            is_draft: false,
            reviewers: Vec::new(),
            commenters: Vec::new(),
            ci_status: None,
            latest_comment: None,
        }
    }

    fn make_notification(thread_id: u64, updated_at: &str) -> NotificationItem {
        NotificationItem {
            thread_id,
            reason: "mention".into(),
            repo: "o/r".into(),
            kind: "pr",
            number: Some(1),
            title: "t".into(),
            url: "https://example.com".into(),
            updated_at: updated_at.into(),
        }
    }

    #[test]
    fn item_key_joins_repo_kind_number() {
        assert_eq!(item_key("o/r", "pr", 7), "o/r:pr:7");
        assert_eq!(item_key("o/r", "issue", 3), "o/r:issue:3");
    }

    #[test]
    fn fresh_notifications_flags_new_thread_ids() {
        let prev = HashMap::new();
        let curr = vec![make_notification(1, "t1")];
        let fresh = fresh_notifications(&prev, &curr);
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].thread_id, 1);
    }

    #[test]
    fn fresh_notifications_flags_updated_at_advance() {
        let mut prev = HashMap::new();
        prev.insert(1, "t1".to_string());
        let curr = vec![make_notification(1, "t2")];
        let fresh = fresh_notifications(&prev, &curr);
        assert_eq!(fresh.len(), 1);
    }

    #[test]
    fn fresh_notifications_skips_unchanged() {
        let mut prev = HashMap::new();
        prev.insert(1, "t1".to_string());
        let curr = vec![make_notification(1, "t1")];
        let fresh = fresh_notifications(&prev, &curr);
        assert!(fresh.is_empty());
    }

    #[test]
    fn describe_item_change_returns_none_when_nothing_changed() {
        let base = make_item(1, "o/r", "pr", 1);
        assert_eq!(describe_item_change(&base, &base.clone()), None);
    }

    #[test]
    fn describe_item_change_reports_state_transitions_first() {
        let base = make_item(1, "o/r", "pr", 1);
        let mut next = base.clone();
        next.state = "merged";
        next.updated_at = "t2".into();
        assert_eq!(
            describe_item_change(&base, &next),
            Some("Now merged".into())
        );
    }

    #[test]
    fn describe_item_change_reports_draft_ready() {
        let mut prev = make_item(1, "o/r", "pr", 1);
        prev.is_draft = true;
        let mut curr = prev.clone();
        curr.is_draft = false;
        curr.updated_at = "t2".into();
        assert_eq!(
            describe_item_change(&prev, &curr),
            Some("Ready for review".into())
        );
    }

    #[test]
    fn describe_item_change_reports_ci_status_change() {
        let mut prev = make_item(1, "o/r", "pr", 1);
        prev.ci_status = Some("pending");
        let mut curr = prev.clone();
        curr.ci_status = Some("failure");
        curr.updated_at = "t2".into();
        assert_eq!(
            describe_item_change(&prev, &curr),
            Some("CI failure".into())
        );
    }

    #[test]
    fn describe_item_change_reports_first_reviewer_state_shift() {
        let mut prev = make_item(1, "o/r", "pr", 1);
        prev.reviewers = vec![Reviewer {
            login: "alice".into(),
            avatar_url: "".into(),
            state: "pending",
        }];
        let mut curr = prev.clone();
        curr.reviewers = vec![Reviewer {
            login: "alice".into(),
            avatar_url: "".into(),
            state: "approved",
        }];
        curr.updated_at = "t2".into();
        assert_eq!(
            describe_item_change(&prev, &curr),
            Some("alice approved".into())
        );
    }

    #[test]
    fn describe_item_change_reports_comment_growth_with_pluralisation() {
        let prev = make_item(1, "o/r", "pr", 1);
        let mut one = prev.clone();
        one.comments = 1;
        one.updated_at = "t2".into();
        assert_eq!(describe_item_change(&prev, &one), Some("+1 comment".into()));

        let mut three = prev.clone();
        three.comments = 3;
        three.updated_at = "t2".into();
        assert_eq!(
            describe_item_change(&prev, &three),
            Some("+3 comments".into())
        );
    }

    #[test]
    fn describe_item_change_falls_back_to_updated() {
        let prev = make_item(1, "o/r", "pr", 1);
        let mut curr = prev.clone();
        curr.updated_at = "t2".into();
        assert_eq!(describe_item_change(&prev, &curr), Some("Updated".into()));
    }

    #[test]
    fn compute_item_changes_flags_new_items() {
        let prev = HashMap::new();
        let item = make_item(1, "o/r", "pr", 1);
        let out = compute_item_changes(&prev, std::slice::from_ref(&item), &HashSet::new());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].reason, "New in list");
    }

    #[test]
    fn compute_item_changes_skips_notified_keys() {
        let item = make_item(1, "o/r", "pr", 1);
        let mut prev = HashMap::new();
        prev.insert(1u64, item.clone());
        let mut next = item.clone();
        next.updated_at = "t2".into();

        let mut notified = HashSet::new();
        notified.insert(item_key("o/r", "pr", 1));

        let out = compute_item_changes(&prev, &[next], &notified);
        assert!(out.is_empty());
    }

    #[test]
    fn compute_item_changes_notified_skip_covers_new_items_too() {
        let item = make_item(1, "o/r", "pr", 1);
        let mut notified = HashSet::new();
        notified.insert(item_key("o/r", "pr", 1));
        let out = compute_item_changes(&HashMap::new(), &[item], &notified);
        assert!(out.is_empty());
    }

    #[test]
    fn removed_items_surfaces_items_that_dropped_out() {
        let prev_item = make_item(1, "o/r", "pr", 1);
        let mut prev = HashMap::new();
        prev.insert(1u64, prev_item.clone());

        let removed = removed_items(&prev, &[], &HashSet::new());
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].id, 1);
    }

    #[test]
    fn removed_items_skips_items_covered_by_notifications() {
        let prev_item = make_item(1, "o/r", "pr", 1);
        let mut prev = HashMap::new();
        prev.insert(1u64, prev_item.clone());

        let mut notified = HashSet::new();
        notified.insert(item_key("o/r", "pr", 1));

        let removed = removed_items(&prev, &[], &notified);
        assert!(removed.is_empty());
    }

    #[test]
    fn removed_items_ignores_items_still_present() {
        let item = make_item(1, "o/r", "pr", 1);
        let mut prev = HashMap::new();
        prev.insert(1u64, item.clone());

        let removed = removed_items(&prev, &[item], &HashSet::new());
        assert!(removed.is_empty());
    }
}
