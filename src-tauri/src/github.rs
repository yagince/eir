use std::sync::Mutex;

use octocrab::models::NotificationId;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::auth::{clear_stored_token, AppState};

/// Error type for the inner fetch helpers that background tasks and Tauri
/// commands share. The `is_unauthorized` flag lets callers decide whether to
/// clear the persisted token (a 401 means the stored credential is dead).
pub struct GithubError {
    pub message: String,
    pub is_unauthorized: bool,
}

impl GithubError {
    pub fn from_octocrab(err: octocrab::Error) -> Self {
        let is_unauthorized = match &err {
            octocrab::Error::GitHub { source, .. } => source.status_code.as_u16() == 401,
            _ => false,
        };
        Self {
            message: err.to_string(),
            is_unauthorized,
        }
    }

    pub fn other(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            is_unauthorized: false,
        }
    }
}

pub fn build_octocrab(token: &str) -> Result<octocrab::Octocrab, String> {
    octocrab::OctocrabBuilder::new()
        .personal_token(token.to_string())
        .build()
        .map_err(|e| e.to_string())
}

fn build_octo(auth: &Mutex<AppState>) -> Result<octocrab::Octocrab, String> {
    let token = auth
        .lock()
        .unwrap()
        .token
        .clone()
        .ok_or("not_authenticated")?;
    build_octocrab(&token)
}

// ---------------------------------------------------------------------------
// fetch_watched — GraphQL search that pulls reviews, reviewRequests and
// CI status rollup in a single request.
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(Debug))]
pub struct Reviewer {
    pub login: String,
    pub avatar_url: String,
    pub state: &'static str, // "approved" | "changes_requested" | "commented" | "dismissed" | "pending"
}

#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(Debug))]
pub struct Commenter {
    pub login: String,
    pub avatar_url: String,
}

#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct LatestComment {
    pub author: String,
    pub author_avatar: String,
    pub body_text: String,
    pub created_at: String,
    pub url: String,
}

#[derive(Serialize, Clone)]
pub struct WatchedItem {
    pub id: u64,
    pub kind: &'static str,
    pub title: String,
    pub number: u64,
    pub repo: String,
    pub url: String,
    pub author: String,
    pub author_avatar: String,
    pub comments: u32,
    pub updated_at: String,
    pub state: &'static str,
    pub is_draft: bool,
    pub reviewers: Vec<Reviewer>,
    pub commenters: Vec<Commenter>,
    pub ci_status: Option<&'static str>, // "success" | "pending" | "failure" | "error"
    pub latest_comment: Option<LatestComment>,
}

fn queries_for_tab(tab: &str, watched_orgs: &[String]) -> Vec<String> {
    match tab {
        "authored" => vec!["is:open is:pr author:@me archived:false".into()],
        // Two queries unioned via the GraphQL alias batching in `fetch_watched`:
        // `review-requested` drops a PR out once you submit any review, so the
        // tab used to "empty itself" after commenting/approving. Adding
        // `reviewed-by` keeps PRs you've been assigned to — and touched —
        // visible until they're closed.
        "review" => vec![
            "is:open is:pr review-requested:@me archived:false".into(),
            "is:open is:pr reviewed-by:@me archived:false".into(),
        ],
        "mentions" => vec!["is:open mentions:@me archived:false".into()],
        _ => {
            let mut qs = vec![
                "is:open involves:@me archived:false".into(),
                "is:open is:pr user:@me archived:false".into(),
            ];
            for org in watched_orgs {
                // Allow-list characters to avoid breaking out of the
                // qualifier. GitHub logins are [A-Za-z0-9-] only.
                let clean: String = org
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                    .collect();
                if clean.is_empty() {
                    continue;
                }
                qs.push(format!("is:open is:pr user:{clean} archived:false"));
            }
            qs
        }
    }
}

const SEARCH_FRAGMENTS: &str = r#"
fragment PrFields on PullRequest {
  databaseId
  title
  number
  url
  updatedAt
  state
  isDraft
  comments(last: 1) {
    totalCount
    nodes {
      author {
        login
        ... on User { avatarUrl }
        ... on Bot { avatarUrl }
      }
      bodyText
      createdAt
      url
    }
  }
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

fragment IssueFields on Issue {
  databaseId
  title
  number
  url
  updatedAt
  state
  comments(last: 20) {
    totalCount
    nodes {
      author {
        login
        ... on User { avatarUrl }
        ... on Bot { avatarUrl }
      }
      bodyText
      createdAt
      url
    }
  }
  repository { nameWithOwner }
  author {
    login
    ... on User { avatarUrl }
    ... on Bot { avatarUrl }
  }
}
"#;

fn build_search_query(n: usize) -> String {
    let mut vars = Vec::with_capacity(n);
    let mut aliases = Vec::with_capacity(n);
    for i in 0..n {
        vars.push(format!("$q{i}: String!"));
        aliases.push(format!(
            "  s{i}: search(query: $q{i}, type: ISSUE, first: 50) {{\n    nodes {{\n      __typename\n      ... on PullRequest {{ ...PrFields }}\n      ... on Issue {{ ...IssueFields }}\n    }}\n  }}"
        ));
    }
    format!(
        "{SEARCH_FRAGMENTS}\nquery Search({vars}) {{\n{aliases}\n}}",
        vars = vars.join(", "),
        aliases = aliases.join("\n"),
    )
}

#[derive(Deserialize)]
struct GqlResp {
    data: Option<std::collections::HashMap<String, SearchResult>>,
    #[serde(default)]
    errors: Vec<GqlError>,
}

#[derive(Deserialize)]
struct GqlError {
    message: String,
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
    comments: IssueCommentsNode,
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
    comments: IssueCommentsNode,
    repository: RepoNode,
    author: Option<AuthorNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IssueCommentsNode {
    total_count: u32,
    #[serde(default)]
    nodes: Vec<Option<IssueCommentNode>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IssueCommentNode {
    author: Option<AuthorNode>,
    #[serde(default)]
    body_text: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    url: String,
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

/// Pick the most recent comment with a real author from a `comments(last: N)`
/// slice. GraphQL returns the slice oldest→newest, so the back of the vec is
/// the newest. Anonymous (deleted-user) comments are skipped — there's no
/// useful "@author" to display.
fn latest_comment_from(nodes: &[Option<IssueCommentNode>]) -> Option<LatestComment> {
    for slot in nodes.iter().rev() {
        let Some(node) = slot else { continue };
        let Some(author) = node.author.as_ref() else {
            continue;
        };
        return Some(LatestComment {
            author: author.login.clone(),
            author_avatar: author.avatar_url.clone().unwrap_or_default(),
            body_text: node.body_text.clone(),
            created_at: node.created_at.clone(),
            url: node.url.clone(),
        });
    }
    None
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

    let latest_comment = latest_comment_from(&pr.comments.nodes);

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
        commenters: Vec::new(),
        ci_status,
        latest_comment,
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

    // Dedupe commenters by login, preserving the order of their first
    // appearance in the (last-N) comments slice. The issue author is kept
    // in the list too — they often drive the discussion and showing only
    // "others" would hide that. The frontend can decide whether to draw
    // the author chip differently.
    let latest_comment = latest_comment_from(&issue.comments.nodes);

    let mut seen_logins: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut commenters: Vec<Commenter> = Vec::new();
    for c in issue.comments.nodes.into_iter().flatten() {
        let Some(user) = c.author else { continue };
        if !seen_logins.insert(user.login.clone()) {
            continue;
        }
        commenters.push(Commenter {
            login: user.login,
            avatar_url: user.avatar_url.unwrap_or_default(),
        });
    }

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
        commenters,
        ci_status: None,
        latest_comment,
    }
}

pub async fn fetch_watched_with(
    octo: &octocrab::Octocrab,
    tab: &str,
    watched_orgs: &[String],
) -> Result<Vec<WatchedItem>, GithubError> {
    let queries = queries_for_tab(tab, watched_orgs);
    let query_str = build_search_query(queries.len());
    let variables: serde_json::Map<String, serde_json::Value> = queries
        .iter()
        .enumerate()
        .map(|(i, q)| (format!("q{i}"), serde_json::Value::String(q.clone())))
        .collect();

    let body = serde_json::json!({
        "query": query_str,
        "variables": variables,
    });

    let resp: GqlResp = octo
        .graphql(&body)
        .await
        .map_err(GithubError::from_octocrab)?;

    if !resp.errors.is_empty() {
        return Err(GithubError::other(
            resp.errors
                .into_iter()
                .map(|e| e.message)
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }

    let Some(data) = resp.data else {
        return Err(GithubError::other("graphql returned no data"));
    };

    // Walk alias results in a stable order (s0, s1, ...) so the primary
    // `involves:@me` query always gets first crack at each id.
    Ok(merge_search_results(data))
}

#[tauri::command]
pub async fn fetch_watched(
    tab: String,
    watched_orgs: Option<Vec<String>>,
    auth: State<'_, Mutex<AppState>>,
) -> Result<Vec<WatchedItem>, String> {
    let octo = build_octo(&auth)?;
    let orgs = watched_orgs.unwrap_or_default();
    match fetch_watched_with(&octo, &tab, &orgs).await {
        Ok(items) => Ok(items),
        Err(err) => {
            if err.is_unauthorized {
                clear_stored_token(&auth);
                return Err("not_authenticated".into());
            }
            Err(err.message)
        }
    }
}

/// Collapse the alias-keyed map returned by GraphQL search (s0, s1, …) into
/// a single deduplicated, updated_at-desc-sorted list of items. Walking the
/// aliases in name order gives the primary `involves:@me` query (s0) first
/// dibs on each id, so the later watched-org queries don't clobber it.
fn merge_search_results(data: std::collections::HashMap<String, SearchResult>) -> Vec<WatchedItem> {
    let mut ordered: Vec<(String, SearchResult)> = data.into_iter().collect();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));

    let mut seen = std::collections::HashSet::new();
    let mut items: Vec<WatchedItem> = Vec::new();
    for (_, result) in ordered {
        for node in result.nodes.into_iter().flatten() {
            let item = match node {
                SearchNode::PullRequest(pr) => Some(pr_to_item(pr)),
                SearchNode::Issue(i) => Some(issue_to_item(i)),
                SearchNode::Unknown => None,
            };
            if let Some(item) = item {
                if seen.insert(item.id) {
                    items.push(item);
                }
            }
        }
    }

    items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    items
}

// ---------------------------------------------------------------------------
// fetch_item_states — look up the final state (open/closed/merged) of a
// list of PRs/Issues by (repo, kind, number). Used to precisely label
// disappearance notifications: when an item drops out of the `is:open`
// search result we want to say "Merged" vs "Closed" rather than guessing.
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
pub struct ItemRef {
    pub repo: String,
    pub kind: String, // "pr" | "issue"
    pub number: u64,
}

#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct ItemState {
    pub repo: String,
    pub kind: &'static str,
    pub number: u64,
    pub state: &'static str, // "open" | "closed" | "merged" | "unknown"
}

/// Build the GraphQL query body (query string + variables) for a batch state
/// lookup. Owner and name go through variables to avoid any string-escape
/// surprises; number is inlined since it's a scalar from trusted data.
fn build_item_states_query(
    items: &[ItemRef],
) -> (String, serde_json::Map<String, serde_json::Value>) {
    let mut var_decls: Vec<String> = Vec::new();
    let mut aliases: Vec<String> = Vec::new();
    let mut variables = serde_json::Map::new();

    for (i, it) in items.iter().enumerate() {
        let Some((owner, name)) = it.repo.split_once('/') else {
            continue;
        };
        let sub = match it.kind.as_str() {
            "pr" => format!("pullRequest(number: {}) {{ state }}", it.number),
            "issue" => format!("issue(number: {}) {{ state }}", it.number),
            _ => continue,
        };
        var_decls.push(format!("$o{i}: String!, $n{i}: String!"));
        aliases.push(format!(
            "  i{i}: repository(owner: $o{i}, name: $n{i}) {{ {sub} }}"
        ));
        variables.insert(
            format!("o{i}"),
            serde_json::Value::String(owner.to_string()),
        );
        variables.insert(format!("n{i}"), serde_json::Value::String(name.to_string()));
    }

    let query = format!(
        "query Q({vars}) {{\n{aliases}\n}}",
        vars = var_decls.join(", "),
        aliases = aliases.join("\n"),
    );
    (query, variables)
}

fn parse_item_state(resp: &serde_json::Value, index: usize, kind: &str) -> &'static str {
    let alias = format!("i{index}");
    let Some(node) = resp.get("data").and_then(|d| d.get(&alias)) else {
        return "unknown";
    };
    let inner_key = match kind {
        "pr" => "pullRequest",
        "issue" => "issue",
        _ => return "unknown",
    };
    let raw = node
        .get(inner_key)
        .and_then(|v| v.get("state"))
        .and_then(|s| s.as_str())
        .unwrap_or("");
    map_item_state(raw)
}

pub async fn fetch_item_states_with(
    octo: &octocrab::Octocrab,
    items: &[ItemRef],
) -> Result<Vec<ItemState>, GithubError> {
    if items.is_empty() {
        return Ok(Vec::new());
    }

    let (query, variables) = build_item_states_query(items);
    let body = serde_json::json!({ "query": query, "variables": variables });

    let resp: serde_json::Value = octo
        .graphql(&body)
        .await
        .map_err(GithubError::from_octocrab)?;

    // Bubble GraphQL-level errors (e.g. a repo we no longer have access to)
    // rather than silently returning "unknown" for every item — that would
    // mask a real failure.
    if let Some(errors) = resp.get("errors").and_then(|e| e.as_array()) {
        if !errors.is_empty() {
            let joined = errors
                .iter()
                .filter_map(|e| e.get("message").and_then(|m| m.as_str()))
                .collect::<Vec<_>>()
                .join("; ");
            if !joined.is_empty() {
                return Err(GithubError::other(joined));
            }
        }
    }

    let mut out = Vec::with_capacity(items.len());
    for (i, it) in items.iter().enumerate() {
        let kind: &'static str = match it.kind.as_str() {
            "pr" => "pr",
            "issue" => "issue",
            _ => continue,
        };
        let state = parse_item_state(&resp, i, kind);
        out.push(ItemState {
            repo: it.repo.clone(),
            kind,
            number: it.number,
            state,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn fetch_item_states(
    items: Vec<ItemRef>,
    auth: State<'_, Mutex<AppState>>,
) -> Result<Vec<ItemState>, String> {
    let octo = build_octo(&auth)?;
    match fetch_item_states_with(&octo, &items).await {
        Ok(states) => Ok(states),
        Err(err) => {
            if err.is_unauthorized {
                clear_stored_token(&auth);
                return Err("not_authenticated".into());
            }
            Err(err.message)
        }
    }
}

// ---------------------------------------------------------------------------
// Notifications — GitHub's GraphQL schema does not expose the notifications
// inbox or mark-as-read, so these remain REST.
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
pub struct NotificationItem {
    pub thread_id: u64,
    pub reason: String,
    pub repo: String,
    pub kind: &'static str,
    pub number: Option<u64>,
    pub title: String,
    pub url: String,
    pub updated_at: String,
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

pub async fn fetch_notifications_with(
    octo: &octocrab::Octocrab,
) -> Result<Vec<NotificationItem>, GithubError> {
    let page = octo
        .activity()
        .notifications()
        .list()
        .all(false)
        .per_page(100)
        .send()
        .await
        .map_err(GithubError::from_octocrab)?;

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
pub async fn fetch_notifications(
    auth: State<'_, Mutex<AppState>>,
) -> Result<Vec<NotificationItem>, String> {
    let octo = build_octo(&auth)?;
    match fetch_notifications_with(&octo).await {
        Ok(items) => Ok(items),
        Err(err) => {
            if err.is_unauthorized {
                clear_stored_token(&auth);
                return Err("not_authenticated".into());
            }
            Err(err.message)
        }
    }
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
            let ge = GithubError::from_octocrab(err);
            if ge.is_unauthorized {
                clear_stored_token(&auth);
                return Err("not_authenticated".into());
            }
            Err(ge.message)
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
    fn queries_for_tab_all_includes_involves_and_user_self() {
        let qs = queries_for_tab("all", &[]);
        assert_eq!(qs.len(), 2);
        assert!(qs[0].contains("involves:@me"));
        assert!(qs[1].contains("user:@me"));
    }

    #[test]
    fn queries_for_tab_all_expands_watched_orgs() {
        let qs = queries_for_tab("all", &["Lecto-inc".to_string(), "other".to_string()]);
        assert_eq!(qs.len(), 4);
        assert!(qs[2].contains("user:Lecto-inc"));
        assert!(qs[3].contains("user:other"));
    }

    #[test]
    fn queries_for_tab_mine_ignores_watched_orgs() {
        let qs = queries_for_tab("authored", &["Lecto-inc".to_string()]);
        assert_eq!(qs.len(), 1);
        assert!(qs[0].contains("author:@me"));
    }

    #[test]
    fn queries_for_tab_review_includes_requested_and_reviewed_by() {
        let qs = queries_for_tab("review", &[]);
        assert_eq!(qs.len(), 2);
        assert!(qs.iter().any(|q| q.contains("review-requested:@me")));
        assert!(qs.iter().any(|q| q.contains("reviewed-by:@me")));
    }

    #[test]
    fn queries_for_tab_review_ignores_watched_orgs() {
        let qs = queries_for_tab("review", &["Lecto-inc".to_string()]);
        assert_eq!(qs.len(), 2);
        assert!(qs.iter().all(|q| !q.contains("user:")));
    }

    #[test]
    fn queries_for_tab_strips_unsafe_chars_from_org() {
        let qs = queries_for_tab("all", &["bad org!".to_string()]);
        // spaces and "!" get filtered out; "badorg" survives
        assert!(qs.last().unwrap().contains("user:badorg"));
    }

    #[test]
    fn queries_for_tab_drops_fully_invalid_org() {
        let qs = queries_for_tab("all", &["!@#$".to_string()]);
        // only the two base queries remain
        assert_eq!(qs.len(), 2);
    }

    #[test]
    fn build_search_query_contains_expected_aliases() {
        let q = build_search_query(3);
        assert!(q.contains("$q0: String!"));
        assert!(q.contains("$q2: String!"));
        assert!(q.contains("s0: search(query: $q0"));
        assert!(q.contains("s2: search(query: $q2"));
        // Fragments are defined once and referenced from aliases
        assert!(q.contains("fragment PrFields on PullRequest"));
        assert!(q.contains("...PrFields"));
    }

    fn make_issue_node(database_id: u64, repo: &str, number: u64) -> IssueNode {
        serde_json::from_value(json!({
            "databaseId": database_id,
            "title": "issue title",
            "number": number,
            "url": format!("https://github.com/{repo}/issues/{number}"),
            "updatedAt": "2026-01-01T00:00:00Z",
            "state": "OPEN",
            "comments": { "totalCount": 0 },
            "repository": { "nameWithOwner": repo },
            "author": { "login": "someone", "avatarUrl": "a" }
        }))
        .unwrap()
    }

    fn make_pr_node(database_id: u64, repo: &str, number: u64, updated_at: &str) -> PrNode {
        serde_json::from_value(json!({
            "databaseId": database_id,
            "title": "pr title",
            "number": number,
            "url": format!("https://github.com/{repo}/pull/{number}"),
            "updatedAt": updated_at,
            "state": "OPEN",
            "comments": { "totalCount": 0 },
            "repository": { "nameWithOwner": repo },
            "author": { "login": "someone", "avatarUrl": "a" },
            "reviews": { "nodes": [] },
            "reviewRequests": { "nodes": [] },
            "commits": { "nodes": [] }
        }))
        .unwrap()
    }

    fn search_result(nodes: Vec<SearchNode>) -> SearchResult {
        SearchResult {
            nodes: nodes.into_iter().map(Some).collect(),
        }
    }

    #[test]
    fn issue_to_item_copies_scalar_fields() {
        let item = issue_to_item(make_issue_node(10, "o/r", 7));
        assert_eq!(item.id, 10);
        assert_eq!(item.kind, "issue");
        assert_eq!(item.number, 7);
        assert_eq!(item.repo, "o/r");
        assert_eq!(item.state, "open");
        assert!(!item.is_draft);
        assert!(item.reviewers.is_empty());
        assert!(item.commenters.is_empty());
        assert!(item.ci_status.is_none());
    }

    fn make_issue_node_with_comments(nodes: serde_json::Value) -> IssueNode {
        serde_json::from_value(json!({
            "databaseId": 1,
            "title": "t",
            "number": 1,
            "url": "https://github.com/o/r/issues/1",
            "updatedAt": "2026-01-01T00:00:00Z",
            "state": "OPEN",
            "comments": { "totalCount": 0, "nodes": nodes },
            "repository": { "nameWithOwner": "o/r" },
            "author": { "login": "author", "avatarUrl": "a" }
        }))
        .unwrap()
    }

    #[test]
    fn issue_commenters_dedupe_by_login_preserving_first_seen_order() {
        let item = issue_to_item(make_issue_node_with_comments(json!([
            { "author": { "login": "alice", "avatarUrl": "a" } },
            { "author": { "login": "bob",   "avatarUrl": "b" } },
            { "author": { "login": "alice", "avatarUrl": "a" } }
        ])));
        let logins: Vec<_> = item.commenters.iter().map(|c| c.login.as_str()).collect();
        assert_eq!(logins, vec!["alice", "bob"]);
    }

    #[test]
    fn issue_commenters_skip_anonymous_and_null_nodes() {
        // A deleted-user GitHub comment surfaces as `author: null`; skip it
        // rather than pushing an empty-login chip.
        let item = issue_to_item(make_issue_node_with_comments(json!([
            { "author": null },
            { "author": { "login": "alice", "avatarUrl": "a" } }
        ])));
        let logins: Vec<_> = item.commenters.iter().map(|c| c.login.as_str()).collect();
        assert_eq!(logins, vec!["alice"]);
    }

    #[test]
    fn issue_commenters_empty_when_no_comments_field_nodes() {
        // Legacy/older backend shape without `nodes` must still deserialize.
        let item = issue_to_item(make_issue_node(10, "o/r", 7));
        assert!(item.commenters.is_empty());
    }

    #[test]
    fn issue_latest_comment_picks_most_recent_node() {
        // GraphQL returns the slice oldest→newest, so the LAST element is the
        // newest. issue_to_item must surface that one as `latest_comment`.
        let item = issue_to_item(make_issue_node_with_comments(json!([
            {
                "author": { "login": "alice", "avatarUrl": "a-url" },
                "bodyText": "first",
                "createdAt": "2026-01-01T00:00:00Z",
                "url": "https://github.com/o/r/issues/1#c1"
            },
            {
                "author": { "login": "bob", "avatarUrl": "b-url" },
                "bodyText": "second is newest",
                "createdAt": "2026-02-01T00:00:00Z",
                "url": "https://github.com/o/r/issues/1#c2"
            }
        ])));
        let lc = item.latest_comment.expect("latest_comment present");
        assert_eq!(lc.author, "bob");
        assert_eq!(lc.author_avatar, "b-url");
        assert_eq!(lc.body_text, "second is newest");
        assert_eq!(lc.created_at, "2026-02-01T00:00:00Z");
        assert_eq!(lc.url, "https://github.com/o/r/issues/1#c2");
    }

    #[test]
    fn issue_latest_comment_skips_anonymous_authors_walking_backwards() {
        // The newest comment was made by a deleted user; we still want a
        // useful "@author:" line, so fall back to the second-newest.
        let item = issue_to_item(make_issue_node_with_comments(json!([
            {
                "author": { "login": "alice", "avatarUrl": "a" },
                "bodyText": "kept",
                "createdAt": "2026-01-01T00:00:00Z",
                "url": "u1"
            },
            { "author": null, "bodyText": "ghost", "createdAt": "2026-02-01T00:00:00Z", "url": "u2" }
        ])));
        let lc = item.latest_comment.expect("falls back to second-newest");
        assert_eq!(lc.author, "alice");
        assert_eq!(lc.body_text, "kept");
    }

    #[test]
    fn issue_latest_comment_is_none_when_no_comments() {
        let item = issue_to_item(make_issue_node(10, "o/r", 7));
        assert!(item.latest_comment.is_none());
    }

    #[test]
    fn pr_latest_comment_uses_last_node() {
        let pr: PrNode = serde_json::from_value(json!({
            "databaseId": 1,
            "title": "t",
            "number": 1,
            "url": "https://github.com/o/r/pull/1",
            "updatedAt": "2026-01-01T00:00:00Z",
            "state": "OPEN",
            "comments": {
                "totalCount": 1,
                "nodes": [
                    {
                        "author": { "login": "carol", "avatarUrl": "c" },
                        "bodyText": "looks good",
                        "createdAt": "2026-04-01T00:00:00Z",
                        "url": "https://github.com/o/r/pull/1#c1"
                    }
                ]
            },
            "repository": { "nameWithOwner": "o/r" },
            "author": { "login": "author", "avatarUrl": "a" },
            "reviews": { "nodes": [] },
            "reviewRequests": { "nodes": [] },
            "commits": { "nodes": [] }
        }))
        .unwrap();
        let item = pr_to_item(pr);
        let lc = item.latest_comment.expect("latest_comment present");
        assert_eq!(lc.author, "carol");
        assert_eq!(lc.body_text, "looks good");
        assert_eq!(lc.url, "https://github.com/o/r/pull/1#c1");
    }

    #[test]
    fn pr_latest_comment_is_none_when_no_nodes() {
        let pr = make_pr(json!([]), json!([]));
        let item = pr_to_item(pr);
        assert!(item.latest_comment.is_none());
    }

    #[test]
    fn merge_search_results_sorts_by_updated_at_desc() {
        let mut data = std::collections::HashMap::new();
        data.insert(
            "s0".to_string(),
            search_result(vec![
                SearchNode::PullRequest(make_pr_node(1, "o/r", 1, "2026-03-01T00:00:00Z")),
                SearchNode::PullRequest(make_pr_node(2, "o/r", 2, "2026-04-01T00:00:00Z")),
            ]),
        );
        let out = merge_search_results(data);
        // Newer first
        assert_eq!(out[0].id, 2);
        assert_eq!(out[1].id, 1);
    }

    #[test]
    fn merge_search_results_dedups_by_id_across_aliases() {
        let mut data = std::collections::HashMap::new();
        // The same PR appears in both queries (e.g. a user's own PR is in
        // involves:@me and user:@me). Later alias must not clobber the one
        // from the primary query.
        data.insert(
            "s0".to_string(),
            search_result(vec![SearchNode::PullRequest(make_pr_node(
                1,
                "o/r",
                1,
                "2026-04-01T00:00:00Z",
            ))]),
        );
        data.insert(
            "s1".to_string(),
            search_result(vec![SearchNode::PullRequest(make_pr_node(
                1,
                "o/r",
                1,
                "2026-04-01T00:00:00Z",
            ))]),
        );
        let out = merge_search_results(data);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, 1);
    }

    #[test]
    fn merge_search_results_mixes_prs_and_issues() {
        let mut data = std::collections::HashMap::new();
        data.insert(
            "s0".to_string(),
            search_result(vec![
                SearchNode::PullRequest(make_pr_node(1, "o/r", 1, "2026-04-02T00:00:00Z")),
                SearchNode::Issue(make_issue_node(2, "o/r", 2)),
            ]),
        );
        let out = merge_search_results(data);
        assert_eq!(out.len(), 2);
        let kinds: Vec<_> = out.iter().map(|i| i.kind).collect();
        assert!(kinds.contains(&"pr"));
        assert!(kinds.contains(&"issue"));
    }

    #[test]
    fn merge_search_results_skips_unknown_nodes() {
        let mut data = std::collections::HashMap::new();
        data.insert(
            "s0".to_string(),
            search_result(vec![
                SearchNode::Unknown,
                SearchNode::PullRequest(make_pr_node(1, "o/r", 1, "2026-04-01T00:00:00Z")),
                SearchNode::Unknown,
            ]),
        );
        let out = merge_search_results(data);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, 1);
    }

    #[test]
    fn merge_search_results_drops_none_slots_in_nodes() {
        // A search page can contain explicit null entries in GraphQL; Option<SearchNode>
        // lets serde deserialize those as None and merge should ignore them.
        let data = std::collections::HashMap::from([(
            "s0".to_string(),
            SearchResult {
                nodes: vec![
                    None,
                    Some(SearchNode::PullRequest(make_pr_node(
                        1,
                        "o/r",
                        1,
                        "2026-04-01T00:00:00Z",
                    ))),
                    None,
                ],
            },
        )]);
        let out = merge_search_results(data);
        assert_eq!(out.len(), 1);
    }

    fn make_item_ref(repo: &str, kind: &str, number: u64) -> ItemRef {
        ItemRef {
            repo: repo.to_string(),
            kind: kind.to_string(),
            number,
        }
    }

    #[test]
    fn build_item_states_query_uses_variables_for_owner_and_name() {
        let (query, variables) = build_item_states_query(&[
            make_item_ref("owner1/repo1", "pr", 10),
            make_item_ref("owner2/repo2", "issue", 20),
        ]);

        // Each item gets its own aliased variables.
        assert!(query.contains("$o0: String!, $n0: String!"));
        assert!(query.contains("$o1: String!, $n1: String!"));
        // Aliased repositories using those variables.
        assert!(query.contains("i0: repository(owner: $o0, name: $n0)"));
        assert!(query.contains("i1: repository(owner: $o1, name: $n1)"));
        // Sub-selection differs by kind.
        assert!(query.contains("pullRequest(number: 10) { state }"));
        assert!(query.contains("issue(number: 20) { state }"));
        // Variables carry the owner/name values.
        assert_eq!(variables.get("o0").and_then(|v| v.as_str()), Some("owner1"));
        assert_eq!(variables.get("n0").and_then(|v| v.as_str()), Some("repo1"));
        assert_eq!(variables.get("o1").and_then(|v| v.as_str()), Some("owner2"));
        assert_eq!(variables.get("n1").and_then(|v| v.as_str()), Some("repo2"));
    }

    #[test]
    fn build_item_states_query_skips_malformed_repo() {
        // A repo without a "/" is defensive-skipped; its slot simply doesn't
        // appear in the query rather than producing a malformed GraphQL.
        let (query, variables) = build_item_states_query(&[
            make_item_ref("no-slash", "pr", 1),
            make_item_ref("owner/repo", "pr", 2),
        ]);
        assert!(!query.contains("$o0"));
        assert!(query.contains("$o1"));
        assert!(!variables.contains_key("o0"));
        assert!(variables.contains_key("o1"));
    }

    #[test]
    fn build_item_states_query_skips_unknown_kind() {
        let (query, _) = build_item_states_query(&[
            make_item_ref("owner/repo", "discussion", 1),
            make_item_ref("owner/repo", "pr", 2),
        ]);
        assert!(!query.contains("$o0"));
        assert!(query.contains("$o1"));
    }

    #[test]
    fn parse_item_state_maps_merged_closed_open() {
        let resp = json!({
            "data": {
                "i0": { "pullRequest": { "state": "MERGED" } },
                "i1": { "pullRequest": { "state": "CLOSED" } },
                "i2": { "issue": { "state": "CLOSED" } },
                "i3": { "issue": { "state": "OPEN" } }
            }
        });
        assert_eq!(parse_item_state(&resp, 0, "pr"), "merged");
        assert_eq!(parse_item_state(&resp, 1, "pr"), "closed");
        assert_eq!(parse_item_state(&resp, 2, "issue"), "closed");
        assert_eq!(parse_item_state(&resp, 3, "issue"), "open");
    }

    #[test]
    fn parse_item_state_returns_unknown_for_missing_alias() {
        let resp = json!({ "data": { "i0": { "pullRequest": { "state": "MERGED" } } } });
        // Alias i99 isn't in the response — don't blow up, just say unknown.
        assert_eq!(parse_item_state(&resp, 99, "pr"), "unknown");
    }

    #[test]
    fn parse_item_state_returns_unknown_for_deleted_repo() {
        // GitHub returns `null` for the repository alias when the repo was
        // deleted or we lost access. The parser should fall back to unknown
        // instead of panicking.
        let resp = json!({ "data": { "i0": null } });
        assert_eq!(parse_item_state(&resp, 0, "pr"), "unknown");
    }

    #[test]
    fn parse_item_state_returns_unknown_for_unknown_kind() {
        let resp = json!({ "data": { "i0": { "pullRequest": { "state": "MERGED" } } } });
        assert_eq!(parse_item_state(&resp, 0, "commit"), "unknown");
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
