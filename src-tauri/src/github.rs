use std::sync::Mutex;

use octocrab::models::NotificationId;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::auth::{clear_stored_token, AppState};

fn is_unauthorized(err: &octocrab::Error) -> bool {
    if let octocrab::Error::GitHub { source, .. } = err {
        return source.status_code.as_u16() == 401;
    }
    false
}

fn build_octo(auth: &Mutex<AppState>) -> Result<octocrab::Octocrab, String> {
    let token = auth
        .lock()
        .unwrap()
        .token
        .clone()
        .ok_or("not_authenticated")?;
    octocrab::OctocrabBuilder::new()
        .personal_token(token)
        .build()
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// fetch_watched — GraphQL search that pulls reviews, reviewRequests and
// CI status rollup in a single request.
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(Debug))]
pub struct Reviewer {
    login: String,
    avatar_url: String,
    state: &'static str, // "approved" | "changes_requested" | "commented" | "dismissed" | "pending"
}

#[derive(Serialize)]
pub struct WatchedItem {
    id: u64,
    kind: &'static str,
    title: String,
    number: u64,
    repo: String,
    url: String,
    author: String,
    author_avatar: String,
    comments: u32,
    updated_at: String,
    state: &'static str,
    is_draft: bool,
    reviewers: Vec<Reviewer>,
    ci_status: Option<&'static str>, // "success" | "pending" | "failure" | "error"
}

fn query_for_tab(tab: &str) -> &'static str {
    match tab {
        "authored" => "is:open is:pr author:@me archived:false",
        "review" => "is:open is:pr review-requested:@me archived:false",
        "mentions" => "is:open mentions:@me archived:false",
        _ => "is:open involves:@me archived:false",
    }
}

const SEARCH_QUERY: &str = r#"
query($q: String!) {
  search(query: $q, type: ISSUE, first: 50) {
    nodes {
      __typename
      ... on PullRequest {
        databaseId
        title
        number
        url
        updatedAt
        state
        isDraft
        comments { totalCount }
        repository { nameWithOwner }
        author {
          login
          ... on User { avatarUrl }
          ... on Bot { avatarUrl }
        }
        reviews(last: 30) {
          nodes {
            state
            author {
              login
              ... on User { avatarUrl }
              ... on Bot { avatarUrl }
            }
          }
        }
        reviewRequests(first: 10) {
          nodes {
            requestedReviewer {
              __typename
              ... on User { login avatarUrl }
            }
          }
        }
        commits(last: 1) {
          nodes {
            commit {
              statusCheckRollup { state }
            }
          }
        }
      }
      ... on Issue {
        databaseId
        title
        number
        url
        updatedAt
        state
        comments { totalCount }
        repository { nameWithOwner }
        author {
          login
          ... on User { avatarUrl }
          ... on Bot { avatarUrl }
        }
      }
    }
  }
}
"#;

#[derive(Deserialize)]
struct GqlResp {
    data: Option<SearchData>,
    #[serde(default)]
    errors: Vec<GqlError>,
}

#[derive(Deserialize)]
struct GqlError {
    message: String,
}

#[derive(Deserialize)]
struct SearchData {
    search: SearchResult,
}

#[derive(Deserialize)]
struct SearchResult {
    nodes: Vec<Option<SearchNode>>,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum SearchNode {
    PullRequest(PrNode),
    Issue(IssueNode),
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrNode {
    database_id: u64,
    title: String,
    number: u64,
    url: String,
    updated_at: String,
    state: String,
    #[serde(default)]
    is_draft: bool,
    comments: CountNode,
    repository: RepoNode,
    author: Option<AuthorNode>,
    reviews: NodeList<ReviewNode>,
    review_requests: NodeList<ReviewRequestNode>,
    commits: NodeList<PrCommitNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IssueNode {
    database_id: u64,
    title: String,
    number: u64,
    url: String,
    updated_at: String,
    state: String,
    comments: CountNode,
    repository: RepoNode,
    author: Option<AuthorNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CountNode {
    total_count: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoNode {
    name_with_owner: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AuthorNode {
    login: String,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct NodeList<T> {
    nodes: Vec<Option<T>>,
}

#[derive(Deserialize)]
struct ReviewNode {
    state: String,
    author: Option<AuthorNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewRequestNode {
    requested_reviewer: Option<RequestedReviewer>,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum RequestedReviewer {
    User(AuthorNode),
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize)]
struct PrCommitNode {
    commit: CommitNode,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitNode {
    status_check_rollup: Option<StatusCheckRollup>,
}

#[derive(Deserialize)]
struct StatusCheckRollup {
    state: String,
}

fn map_item_state(raw: &str) -> &'static str {
    match raw {
        "OPEN" => "open",
        "CLOSED" => "closed",
        "MERGED" => "merged",
        _ => "unknown",
    }
}

fn map_ci_state(raw: &str) -> &'static str {
    match raw {
        "SUCCESS" => "success",
        "PENDING" | "EXPECTED" => "pending",
        "FAILURE" => "failure",
        "ERROR" => "error",
        _ => "unknown",
    }
}

fn pr_to_item(pr: PrNode) -> WatchedItem {
    let author_login = pr
        .author
        .as_ref()
        .map(|a| a.login.clone())
        .unwrap_or_default();
    let author_avatar = pr
        .author
        .as_ref()
        .and_then(|a| a.avatar_url.clone())
        .unwrap_or_default();

    // Latest substantive review state per user. "Commented" is preserved
    // only if there's no stronger signal (approved / changes_requested /
    // dismissed) yet. A reviewer who approved then left a plain comment
    // still reads as "approved", matching GitHub's own review UI.
    let mut latest: std::collections::HashMap<String, (String, &'static str)> =
        std::collections::HashMap::new();
    for r in pr.reviews.nodes.into_iter().flatten() {
        let Some(user) = r.author else { continue };
        let avatar = user.avatar_url.unwrap_or_default();
        match r.state.as_str() {
            "APPROVED" => {
                latest.insert(user.login, (avatar, "approved"));
            }
            "CHANGES_REQUESTED" => {
                latest.insert(user.login, (avatar, "changes_requested"));
            }
            "DISMISSED" => {
                latest.insert(user.login, (avatar, "dismissed"));
            }
            "COMMENTED" => {
                latest.entry(user.login).or_insert((avatar, "commented"));
            }
            _ => {} // PENDING and anything else
        }
    }

    // If a reviewer is in review_requests it means GitHub is currently
    // asking them to review (either they've never reviewed, or their
    // previous review was dismissed and a fresh one is expected). That
    // "pending" stance overrides a stale "dismissed" — otherwise we'd
    // show someone as dismissed when they're really waiting to review.
    for rr in pr.review_requests.nodes.into_iter().flatten() {
        if let Some(RequestedReviewer::User(u)) = rr.requested_reviewer {
            let avatar = u.avatar_url.unwrap_or_default();
            match latest.get(&u.login).map(|(_, s)| *s) {
                None | Some("dismissed") => {
                    latest.insert(u.login, (avatar, "pending"));
                }
                _ => {} // already has a substantive stance — keep it
            }
        }
    }

    let reviewers: Vec<Reviewer> = latest
        .into_iter()
        .map(|(login, (avatar_url, state))| Reviewer {
            login,
            avatar_url,
            state,
        })
        .collect();

    let ci_status = pr
        .commits
        .nodes
        .into_iter()
        .flatten()
        .next()
        .and_then(|c| c.commit.status_check_rollup)
        .map(|r| map_ci_state(&r.state));

    WatchedItem {
        id: pr.database_id,
        kind: "pr",
        title: pr.title,
        number: pr.number,
        repo: pr.repository.name_with_owner,
        url: pr.url,
        author: author_login,
        author_avatar,
        comments: pr.comments.total_count,
        updated_at: pr.updated_at,
        state: map_item_state(&pr.state),
        is_draft: pr.is_draft,
        reviewers,
        ci_status,
    }
}

fn issue_to_item(issue: IssueNode) -> WatchedItem {
    let author_login = issue
        .author
        .as_ref()
        .map(|a| a.login.clone())
        .unwrap_or_default();
    let author_avatar = issue
        .author
        .as_ref()
        .and_then(|a| a.avatar_url.clone())
        .unwrap_or_default();

    WatchedItem {
        id: issue.database_id,
        kind: "issue",
        title: issue.title,
        number: issue.number,
        repo: issue.repository.name_with_owner,
        url: issue.url,
        author: author_login,
        author_avatar,
        comments: issue.comments.total_count,
        updated_at: issue.updated_at,
        state: map_item_state(&issue.state),
        is_draft: false,
        reviewers: Vec::new(),
        ci_status: None,
    }
}

#[tauri::command]
pub async fn fetch_watched(
    tab: String,
    auth: State<'_, Mutex<AppState>>,
) -> Result<Vec<WatchedItem>, String> {
    let octo = build_octo(&auth)?;

    let body = serde_json::json!({
        "query": SEARCH_QUERY,
        "variables": { "q": query_for_tab(&tab) },
    });

    let resp: GqlResp = match octo.graphql(&body).await {
        Ok(r) => r,
        Err(err) => {
            if is_unauthorized(&err) {
                clear_stored_token(&auth);
                return Err("not_authenticated".into());
            }
            return Err(err.to_string());
        }
    };

    if !resp.errors.is_empty() {
        return Err(resp
            .errors
            .into_iter()
            .map(|e| e.message)
            .collect::<Vec<_>>()
            .join("; "));
    }

    let Some(data) = resp.data else {
        return Err("graphql returned no data".into());
    };

    let mut items: Vec<WatchedItem> = data
        .search
        .nodes
        .into_iter()
        .flatten()
        .filter_map(|node| match node {
            SearchNode::PullRequest(pr) => Some(pr_to_item(pr)),
            SearchNode::Issue(i) => Some(issue_to_item(i)),
            SearchNode::Unknown => None,
        })
        .collect();

    // GraphQL search has no server-side sort for ISSUE type; order by
    // updated_at descending so the UI mirrors the previous REST behaviour.
    items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    Ok(items)
}

// ---------------------------------------------------------------------------
// Notifications — GitHub's GraphQL schema does not expose the notifications
// inbox or mark-as-read, so these remain REST.
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct NotificationItem {
    thread_id: u64,
    reason: String,
    repo: String,
    kind: &'static str,
    number: Option<u64>,
    title: String,
    url: String,
    updated_at: String,
}

fn subject_kind(type_name: &str) -> &'static str {
    match type_name {
        "PullRequest" => "pr",
        "Issue" => "issue",
        "Commit" => "commit",
        "Discussion" => "discussion",
        "Release" => "release",
        _ => "other",
    }
}

fn extract_number(api_url: Option<&str>) -> Option<u64> {
    api_url?.rsplit('/').next()?.parse().ok()
}

fn subject_html_url(api_url: Option<&str>, repo: &str, kind: &str, number: Option<u64>) -> String {
    if let (Some(n), "pr") = (number, kind) {
        return format!("https://github.com/{repo}/pull/{n}");
    }
    if let (Some(n), "issue") = (number, kind) {
        return format!("https://github.com/{repo}/issues/{n}");
    }
    api_url.map(String::from).unwrap_or_default()
}

#[tauri::command]
pub async fn fetch_notifications(
    auth: State<'_, Mutex<AppState>>,
) -> Result<Vec<NotificationItem>, String> {
    let octo = build_octo(&auth)?;

    let page = match octo
        .activity()
        .notifications()
        .list()
        .all(false)
        .per_page(100)
        .send()
        .await
    {
        Ok(p) => p,
        Err(err) => {
            if is_unauthorized(&err) {
                clear_stored_token(&auth);
                return Err("not_authenticated".into());
            }
            return Err(err.to_string());
        }
    };

    let notifications = page
        .items
        .into_iter()
        .map(|n| {
            let kind = subject_kind(&n.subject.r#type);
            let url_str = n.subject.url.as_ref().map(|u| u.as_str());
            let number = extract_number(url_str);
            let repo = n.repository.full_name.clone().unwrap_or_default();
            let url = subject_html_url(url_str, &repo, kind, number);
            NotificationItem {
                thread_id: n.id.0,
                reason: n.reason,
                repo,
                kind,
                number,
                title: n.subject.title,
                url,
                updated_at: n.updated_at.to_rfc3339(),
            }
        })
        .collect();

    Ok(notifications)
}

#[tauri::command]
pub async fn mark_notification_read(
    thread_id: u64,
    auth: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let octo = build_octo(&auth)?;

    match octo
        .activity()
        .notifications()
        .mark_as_read(NotificationId(thread_id))
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => {
            if is_unauthorized(&err) {
                clear_stored_token(&auth);
                return Err("not_authenticated".into());
            }
            Err(err.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn subject_kind_maps_known_types() {
        assert_eq!(subject_kind("PullRequest"), "pr");
        assert_eq!(subject_kind("Issue"), "issue");
        assert_eq!(subject_kind("Commit"), "commit");
        assert_eq!(subject_kind("Discussion"), "discussion");
        assert_eq!(subject_kind("Release"), "release");
        assert_eq!(subject_kind("Something"), "other");
    }

    #[test]
    fn extract_number_parses_last_path_segment() {
        assert_eq!(
            extract_number(Some("https://api.github.com/repos/o/r/pulls/123")),
            Some(123)
        );
        assert_eq!(
            extract_number(Some("https://api.github.com/repos/o/r/issues/7")),
            Some(7)
        );
        assert_eq!(extract_number(None), None);
        assert_eq!(extract_number(Some("")), None);
        assert_eq!(extract_number(Some("nonsense")), None);
    }

    #[test]
    fn subject_html_url_builds_web_urls_for_pr_and_issue() {
        assert_eq!(
            subject_html_url(
                Some("https://api.github.com/repos/owner/repo/pulls/42"),
                "owner/repo",
                "pr",
                Some(42),
            ),
            "https://github.com/owner/repo/pull/42"
        );
        assert_eq!(
            subject_html_url(
                Some("https://api.github.com/repos/owner/repo/issues/99"),
                "owner/repo",
                "issue",
                Some(99),
            ),
            "https://github.com/owner/repo/issues/99"
        );
    }

    #[test]
    fn subject_html_url_falls_back_to_api_url() {
        assert_eq!(
            subject_html_url(Some("https://example.com/x"), "o/r", "commit", None),
            "https://example.com/x"
        );
        assert_eq!(subject_html_url(None, "o/r", "other", None), "");
    }

    #[test]
    fn map_item_state_handles_all_cases() {
        assert_eq!(map_item_state("OPEN"), "open");
        assert_eq!(map_item_state("CLOSED"), "closed");
        assert_eq!(map_item_state("MERGED"), "merged");
        assert_eq!(map_item_state("WHAT"), "unknown");
    }

    #[test]
    fn map_ci_state_handles_all_cases() {
        assert_eq!(map_ci_state("SUCCESS"), "success");
        assert_eq!(map_ci_state("PENDING"), "pending");
        assert_eq!(map_ci_state("EXPECTED"), "pending");
        assert_eq!(map_ci_state("FAILURE"), "failure");
        assert_eq!(map_ci_state("ERROR"), "error");
        assert_eq!(map_ci_state(""), "unknown");
    }

    fn make_pr(reviews: serde_json::Value, review_requests: serde_json::Value) -> PrNode {
        serde_json::from_value(json!({
            "databaseId": 1,
            "title": "t",
            "number": 1,
            "url": "https://github.com/o/r/pull/1",
            "updatedAt": "2026-01-01T00:00:00Z",
            "state": "OPEN",
            "comments": { "totalCount": 0 },
            "repository": { "nameWithOwner": "o/r" },
            "author": { "login": "author", "avatarUrl": "a" },
            "reviews": { "nodes": reviews },
            "reviewRequests": { "nodes": review_requests },
            "commits": { "nodes": [] }
        }))
        .expect("valid PrNode")
    }

    fn find_reviewer<'a>(reviewers: &'a [Reviewer], login: &str) -> &'a Reviewer {
        reviewers
            .iter()
            .find(|r| r.login == login)
            .unwrap_or_else(|| panic!("no reviewer {login} in {reviewers:?}"))
    }

    #[test]
    fn reviewers_approved_overrides_commented() {
        let pr = make_pr(
            json!([
                { "state": "COMMENTED", "author": { "login": "alice", "avatarUrl": "a" } },
                { "state": "APPROVED",  "author": { "login": "alice", "avatarUrl": "a" } }
            ]),
            json!([]),
        );
        let item = pr_to_item(pr);
        assert_eq!(find_reviewer(&item.reviewers, "alice").state, "approved");
    }

    #[test]
    fn reviewers_commented_does_not_override_approved() {
        let pr = make_pr(
            json!([
                { "state": "APPROVED",  "author": { "login": "alice", "avatarUrl": "a" } },
                { "state": "COMMENTED", "author": { "login": "alice", "avatarUrl": "a" } }
            ]),
            json!([]),
        );
        let item = pr_to_item(pr);
        assert_eq!(find_reviewer(&item.reviewers, "alice").state, "approved");
    }

    #[test]
    fn reviewers_latest_substantive_wins() {
        let pr = make_pr(
            json!([
                { "state": "CHANGES_REQUESTED", "author": { "login": "alice", "avatarUrl": "a" } },
                { "state": "APPROVED",          "author": { "login": "alice", "avatarUrl": "a" } }
            ]),
            json!([]),
        );
        let item = pr_to_item(pr);
        assert_eq!(find_reviewer(&item.reviewers, "alice").state, "approved");
    }

    #[test]
    fn reviewers_commented_only_shows_as_commented() {
        let pr = make_pr(
            json!([
                { "state": "COMMENTED", "author": { "login": "alice", "avatarUrl": "a" } }
            ]),
            json!([]),
        );
        let item = pr_to_item(pr);
        assert_eq!(find_reviewer(&item.reviewers, "alice").state, "commented");
    }

    #[test]
    fn reviewers_dismissed_without_rerequest_stays_dismissed() {
        let pr = make_pr(
            json!([
                { "state": "APPROVED",  "author": { "login": "alice", "avatarUrl": "a" } },
                { "state": "DISMISSED", "author": { "login": "alice", "avatarUrl": "a" } }
            ]),
            json!([]),
        );
        let item = pr_to_item(pr);
        assert_eq!(find_reviewer(&item.reviewers, "alice").state, "dismissed");
    }

    #[test]
    fn reviewers_dismissed_plus_rerequest_becomes_pending() {
        let pr = make_pr(
            json!([
                { "state": "DISMISSED", "author": { "login": "alice", "avatarUrl": "a" } }
            ]),
            json!([
                {
                    "requestedReviewer": {
                        "__typename": "User",
                        "login": "alice",
                        "avatarUrl": "a"
                    }
                }
            ]),
        );
        let item = pr_to_item(pr);
        assert_eq!(find_reviewer(&item.reviewers, "alice").state, "pending");
    }

    #[test]
    fn reviewers_approved_plus_rerequest_stays_approved() {
        let pr = make_pr(
            json!([
                { "state": "APPROVED", "author": { "login": "alice", "avatarUrl": "a" } }
            ]),
            json!([
                {
                    "requestedReviewer": {
                        "__typename": "User",
                        "login": "alice",
                        "avatarUrl": "a"
                    }
                }
            ]),
        );
        let item = pr_to_item(pr);
        assert_eq!(find_reviewer(&item.reviewers, "alice").state, "approved");
    }

    #[test]
    fn reviewers_review_request_without_any_review_is_pending() {
        let pr = make_pr(
            json!([]),
            json!([
                {
                    "requestedReviewer": {
                        "__typename": "User",
                        "login": "bob",
                        "avatarUrl": "b"
                    }
                }
            ]),
        );
        let item = pr_to_item(pr);
        assert_eq!(find_reviewer(&item.reviewers, "bob").state, "pending");
    }

    #[test]
    fn reviewers_team_requests_are_ignored_for_now() {
        let pr = make_pr(
            json!([]),
            json!([
                {
                    "requestedReviewer": {
                        "__typename": "Team",
                        "name": "backend"
                    }
                }
            ]),
        );
        let item = pr_to_item(pr);
        assert!(item.reviewers.is_empty());
    }

    #[test]
    fn pr_to_item_copies_scalar_fields() {
        let pr = make_pr(json!([]), json!([]));
        let item = pr_to_item(pr);
        assert_eq!(item.id, 1);
        assert_eq!(item.kind, "pr");
        assert_eq!(item.number, 1);
        assert_eq!(item.repo, "o/r");
        assert_eq!(item.state, "open");
        assert_eq!(item.author, "author");
        assert!(!item.is_draft);
    }

    #[test]
    fn pr_to_item_surfaces_draft_flag() {
        let pr: PrNode = serde_json::from_value(json!({
            "databaseId": 1,
            "title": "t",
            "number": 1,
            "url": "https://github.com/o/r/pull/1",
            "updatedAt": "2026-01-01T00:00:00Z",
            "state": "OPEN",
            "isDraft": true,
            "comments": { "totalCount": 0 },
            "repository": { "nameWithOwner": "o/r" },
            "author": { "login": "author", "avatarUrl": "a" },
            "reviews": { "nodes": [] },
            "reviewRequests": { "nodes": [] },
            "commits": { "nodes": [] }
        }))
        .unwrap();
        let item = pr_to_item(pr);
        assert!(item.is_draft);
    }
}
