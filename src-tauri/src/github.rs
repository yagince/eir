use std::sync::Mutex;

use octocrab::models::IssueState;
use serde::Serialize;
use tauri::State;

use crate::auth::{clear_stored_token, AppState};

fn is_unauthorized(err: &octocrab::Error) -> bool {
    if let octocrab::Error::GitHub { source, .. } = err {
        return source.status_code.as_u16() == 401;
    }
    false
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
    let token = auth
        .lock()
        .unwrap()
        .token
        .clone()
        .ok_or("not_authenticated")?;

    let octo = octocrab::OctocrabBuilder::new()
        .personal_token(token)
        .build()
        .map_err(|e| e.to_string())?;

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
                author: issue.user.login,
                updated_at: issue.updated_at.to_rfc3339(),
                state,
            }
        })
        .collect();

    Ok(items)
}
