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
pub struct Reviewer {
    login: String,
    avatar_url: String,
    state: &'static str, // "approved" | "changes_requested" | "pending"
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
