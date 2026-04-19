use std::sync::Mutex;

use octocrab::models::{IssueState, NotificationId};
use serde::Serialize;
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
}

fn query_for_tab(tab: &str) -> &'static str {
    match tab {
        "authored" => "is:open is:pr author:@me archived:false",
        "review" => "is:open is:pr review-requested:@me archived:false",
        "mentions" => "is:open mentions:@me archived:false",
        _ => "is:open involves:@me archived:false",
    }
}

#[tauri::command]
pub async fn fetch_watched(
    tab: String,
    auth: State<'_, Mutex<AppState>>,
) -> Result<Vec<WatchedItem>, String> {
    let octo = build_octo(&auth)?;

    let page = match octo
        .search()
        .issues_and_pull_requests(query_for_tab(&tab))
        .sort("updated")
        .order("desc")
        .per_page(50)
        .send()
        .await
    {
        Ok(page) => page,
        Err(err) => {
            if is_unauthorized(&err) {
                clear_stored_token(&auth);
                return Err("not_authenticated".into());
            }
            return Err(err.to_string());
        }
    };

    let items = page
        .items
        .into_iter()
        .map(|issue| {
            let repo = issue
                .repository_url
                .path()
                .trim_start_matches("/repos/")
                .to_string();
            let kind = if issue.pull_request.is_some() {
                "pr"
            } else {
                "issue"
            };
            let state = match issue.state {
                IssueState::Open => "open",
                IssueState::Closed => "closed",
                _ => "unknown",
            };
            WatchedItem {
                id: issue.id.0,
                kind,
                title: issue.title,
                number: issue.number,
                repo,
                url: issue.html_url.to_string(),
                author: issue.user.login.clone(),
                author_avatar: issue.user.avatar_url.to_string(),
                comments: issue.comments,
                updated_at: issue.updated_at.to_rfc3339(),
                state,
            }
        })
        .collect();

    Ok(items)
}

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

/// GitHub notification subject URLs are API URLs like
/// `https://api.github.com/repos/OWNER/REPO/pulls/123`. Map back to the
/// web URL the user actually wants to open.
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
