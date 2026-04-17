use std::{future::Future, path::Path, pin::Pin, sync::Arc};

use chrono::{DateTime, Duration, Local};
use google_tasks1::{
    api::Task,
    hyper_rustls::{self, HttpsConnectorBuilder},
    hyper_util,
    yup_oauth2::authenticator_delegate::{DefaultInstalledFlowDelegate, InstalledFlowDelegate},
    yup_oauth2::{self, ApplicationSecret, InstalledFlowReturnMethod},
    TasksHub,
};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use parking_lot::Mutex;

use crate::{settings::InnerSettings, MyApp};

const GOOGLE_CLIENT_SECRET_PATH: &str = "google_client_secret.json";
const GOOGLE_TOKEN_CACHE_PATH: &str = "google_token_cache.json";
const TASKS_READONLY_SCOPE: &str = "https://www.googleapis.com/auth/tasks.readonly";
const LOGIN_TIMEOUT_SECONDS: u64 = 120;
/// JSON content of the embedded Google OAuth client secret, baked in at compile time by build.rs.
/// Falls back to an empty string when the source file is absent so the binary still compiles.
const EMBEDDED_GOOGLE_CLIENT_SECRET_JSON: &str = match option_env!("GOOGLE_CLIENT_SECRET_EMBEDDED")
{
    Some(s) => s,
    None => "",
};

#[derive(Debug, Clone, Default)]
pub struct TaskListItem {
    pub title: String,
    pub due: Option<DateTime<Local>>,
}

#[derive(Debug, Clone)]
pub struct TasksState {
    pub items: Vec<TaskListItem>,
    pub last_sync: Option<DateTime<Local>>,
    pub last_error: Option<String>,
    pub next_sync: DateTime<Local>,
    pub sync_in_flight: bool,
    pub auth_state: TasksAuthState,
    pub auth_request_id: u64,
}

impl Default for TasksState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            last_sync: None,
            last_error: None,
            next_sync: Local::now(),
            sync_in_flight: false,
            auth_state: TasksAuthState::SignedOut,
            auth_request_id: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TasksAuthState {
    SignedOut,
    AwaitingCallback { started_at: DateTime<Local> },
    Authenticated { last_token_refresh: DateTime<Local> },
    AuthCancelled,
    AuthTimedOut,
    AuthError { message: String },
}

/// Returns whether either a local override or embedded OAuth client configuration is available.
pub fn has_oauth_client_config() -> bool {
    Path::new(GOOGLE_CLIENT_SECRET_PATH).exists()
        || serde_json::from_str::<ApplicationSecret>(EMBEDDED_GOOGLE_CLIENT_SECRET_JSON).is_ok()
}

/// Returns a short description of where the active OAuth client configuration comes from.
pub fn oauth_client_source_label() -> &'static str {
    if Path::new(GOOGLE_CLIENT_SECRET_PATH).exists() {
        "Using local OAuth client override"
    } else {
        "Using embedded shared OAuth client"
    }
}

/// Returns the Google Tasks web URL used when opening the tasks page from the sidebar.
pub fn tasks_browser_url(_settings: &InnerSettings) -> &'static str {
    "https://tasks.google.com/tasks/"
}

#[derive(Copy, Clone)]
struct InstalledFlowBrowserDelegate;

impl InstalledFlowDelegate for InstalledFlowBrowserDelegate {
    fn present_user_url<'a>(
        &'a self,
        url: &'a str,
        need_code: bool,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(present_browser_url(url, need_code))
    }
}

/// Initializes auth state based on local OAuth files so startup can reuse cached login.
pub fn bootstrap_auth_state(appdata: &mut MyApp) {
    let mut state = appdata.tasks_state.lock();
    if !has_oauth_client_config() {
        state.auth_state = TasksAuthState::AuthError {
            message: "No usable OAuth client configuration found".to_string(),
        };
        return;
    }

    if Path::new(GOOGLE_TOKEN_CACHE_PATH).exists() {
        state.auth_state = TasksAuthState::Authenticated {
            last_token_refresh: Local::now(),
        };
    } else {
        state.auth_state = TasksAuthState::SignedOut;
    }
}

/// Starts a background sync when enabled and due, while keeping the render path non-blocking.
pub fn schedule_tasks_refresh(appdata: &mut MyApp) {
    let settings = appdata.settings.lock().current_settings.clone();
    if !settings.tasks_enabled {
        return;
    }

    let now = Local::now();

    {
        let mut state = appdata.tasks_state.lock();
        if !matches!(state.auth_state, TasksAuthState::Authenticated { .. }) {
            return;
        }
        if state.sync_in_flight || now < state.next_sync {
            return;
        }
        state.sync_in_flight = true;
    }

    let state_arc = appdata.tasks_state.clone();
    appdata.rt.spawn(async move {
        let refresh_seconds = settings.tasks_refresh_seconds.max(15);
        let sync_result = fetch_tasks(&settings).await;

        let mut state = state_arc.lock();
        state.sync_in_flight = false;
        state.next_sync = Local::now() + Duration::seconds(refresh_seconds as i64);

        match sync_result {
            Ok(items) => {
                state.items = items;
                state.last_sync = Some(Local::now());
                state.last_error = None;
                state.auth_state = TasksAuthState::Authenticated {
                    last_token_refresh: Local::now(),
                };
            }
            Err(err) => {
                state.last_error = Some(err);
            }
        }
    });
}

/// Starts interactive OAuth sign-in and transitions auth state while keeping UI responsive.
pub fn start_sign_in(appdata: &mut MyApp) {
    let request_id = {
        let mut state = appdata.tasks_state.lock();
        if matches!(state.auth_state, TasksAuthState::AwaitingCallback { .. }) {
            return;
        }

        state.auth_request_id = state.auth_request_id.saturating_add(1);
        state.auth_state = TasksAuthState::AwaitingCallback {
            started_at: Local::now(),
        };
        state.auth_request_id
    };

    let state_arc = appdata.tasks_state.clone();
    appdata.rt.spawn(async move {
        let login_result = tokio::time::timeout(
            std::time::Duration::from_secs(LOGIN_TIMEOUT_SECONDS),
            interactive_sign_in(),
        )
        .await;

        let mut state = state_arc.lock();
        if state.auth_request_id != request_id {
            return;
        }

        match login_result {
            Ok(Ok(())) => {
                state.auth_state = TasksAuthState::Authenticated {
                    last_token_refresh: Local::now(),
                };
                state.last_error = None;
                state.next_sync = Local::now();
            }
            Ok(Err(err)) => {
                state.auth_state = TasksAuthState::AuthError {
                    message: err.clone(),
                };
                state.last_error = Some(err);
            }
            Err(_) => {
                state.auth_state = TasksAuthState::AuthTimedOut;
                state.last_error = Some("Login timed out. Try again.".to_string());
            }
        }
    });
}

/// Cancels active login flow state; if an older login result arrives it will be ignored.
pub fn cancel_sign_in(state: &Arc<Mutex<TasksState>>) {
    let mut state = state.lock();
    if matches!(state.auth_state, TasksAuthState::AwaitingCallback { .. }) {
        state.auth_request_id = state.auth_request_id.saturating_add(1);
        state.auth_state = TasksAuthState::AuthCancelled;
        state.last_error = Some("Login cancelled.".to_string());
    }
}

/// Signs out by removing cached token and clearing in-memory task/auth state.
pub fn sign_out(state: &Arc<Mutex<TasksState>>) {
    let mut state = state.lock();
    let _ = std::fs::remove_file(GOOGLE_TOKEN_CACHE_PATH);
    state.items.clear();
    state.last_sync = None;
    state.sync_in_flight = false;
    state.auth_request_id = state.auth_request_id.saturating_add(1);
    state.auth_state = TasksAuthState::SignedOut;
    state.last_error = None;
}

/// Returns true if the auth state currently expects an interactive callback.
pub fn is_awaiting_callback(state: &Arc<Mutex<TasksState>>) -> bool {
    matches!(
        state.lock().auth_state,
        TasksAuthState::AwaitingCallback { .. }
    )
}

/// Fetches tasks from Google Tasks API, filters completed tasks, and sorts by due date ascending.
async fn fetch_tasks(settings: &InnerSettings) -> Result<Vec<TaskListItem>, String> {
    let auth = build_authenticator(
        Path::new(GOOGLE_CLIENT_SECRET_PATH),
        Path::new(GOOGLE_TOKEN_CACHE_PATH),
    )
    .await?;

    let client = build_http_client()?;
    let hub = TasksHub::new(client, auth);

    let list_id = if settings.tasks_list_id.trim().is_empty() {
        "@default"
    } else {
        settings.tasks_list_id.as_str()
    };

    let (_, response) = hub
        .tasks()
        .list(list_id)
        .show_completed(false)
        .show_hidden(false)
        .show_deleted(false)
        .max_results(settings.tasks_max_items.max(1) as i32)
        .doit()
        .await
        .map_err(|e| format!("tasks.list failed: {e}"))?;

    let mut items = Vec::new();
    let source_items = response.items.unwrap_or_default();
    for task in source_items {
        if is_completed(&task) {
            continue;
        }

        let title = task
            .title
            .as_deref()
            .unwrap_or("(untitled task)")
            .trim()
            .to_string();
        if title.is_empty() {
            continue;
        }

        let due = parse_due(&task);
        items.push(TaskListItem { title, due });
    }

    items.sort_by(|a, b| match (a.due, b.due) {
        (Some(lhs), Some(rhs)) => lhs.cmp(&rhs),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.title.cmp(&b.title),
    });

    items.truncate(settings.tasks_max_items.max(1));
    Ok(items)
}

/// Performs interactive login and forces token acquisition to complete callback flow.
async fn interactive_sign_in() -> Result<(), String> {
    let auth = build_authenticator(
        Path::new(GOOGLE_CLIENT_SECRET_PATH),
        Path::new(GOOGLE_TOKEN_CACHE_PATH),
    )
    .await?;

    auth.token(&[TASKS_READONLY_SCOPE])
        .await
        .map_err(|e| format!("OAuth token exchange failed: {e}"))?;

    Ok(())
}

/// Opens the OAuth URL in the user's default browser and falls back to default delegate guidance.
async fn present_browser_url(url: &str, need_code: bool) -> Result<String, String> {
    let open_result = webbrowser::open(url);
    let default_delegate = DefaultInstalledFlowDelegate;

    match open_result {
        Ok(_) => default_delegate.present_user_url(url, need_code).await,
        Err(err) => {
            println!("Failed to open browser automatically: {err}");
            default_delegate.present_user_url(url, need_code).await
        }
    }
}

/// Builds a Google OAuth authenticator and persists refresh tokens to disk.
async fn build_authenticator(
    client_secret_path: &Path,
    token_cache_path: &Path,
) -> Result<
    yup_oauth2::authenticator::Authenticator<hyper_rustls::HttpsConnector<HttpConnector>>,
    String,
> {
    let secret = load_application_secret(client_secret_path).await?;

    yup_oauth2::InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
        .persist_tokens_to_disk(token_cache_path)
        .flow_delegate(Box::new(InstalledFlowBrowserDelegate))
        .build()
        .await
        .map_err(|e| format!("Failed to initialize OAuth flow: {e}"))
}

/// Loads the OAuth client configuration from local override if present, otherwise from embedded app defaults.
async fn load_application_secret(client_secret_path: &Path) -> Result<ApplicationSecret, String> {
    if client_secret_path.exists() {
        return yup_oauth2::read_application_secret(client_secret_path)
            .await
            .map_err(|e| format!("Failed to read local OAuth client secret: {e}"));
    }

    serde_json::from_str(EMBEDDED_GOOGLE_CLIENT_SECRET_JSON)
        .map_err(|e| format!("Failed to parse embedded OAuth client secret: {e}"))
}

/// Creates the HTTPS client required by generated Google API clients.
fn build_http_client(
) -> Result<Client<hyper_rustls::HttpsConnector<HttpConnector>, google_tasks1::common::Body>, String>
{
    let https = HttpsConnectorBuilder::new()
        .with_native_roots()
        .map_err(|e| format!("Failed to load native TLS roots: {e}"))?
        .https_or_http()
        .enable_http1()
        .build();

    Ok(Client::builder(TokioExecutor::new()).build(https))
}

/// Determines whether a Google task is marked as completed.
fn is_completed(task: &Task) -> bool {
    task.status.as_deref() == Some("completed")
}

/// Parses the due timestamp from Google Tasks into local time.
fn parse_due(task: &Task) -> Option<DateTime<Local>> {
    let due = task.due.as_deref()?;
    chrono::DateTime::parse_from_rfc3339(due)
        .ok()
        .map(|d| d.with_timezone(&Local))
}

/// Produces short status text used by the UI for sync state and errors.
pub fn tasks_status_line(state: &Arc<Mutex<TasksState>>) -> String {
    let state = state.lock();
    match &state.auth_state {
        TasksAuthState::SignedOut => {
            return "Sign in required".to_string();
        }
        TasksAuthState::AwaitingCallback { started_at } => {
            return format!(
                "Waiting for Google login... ({}s)",
                (Local::now() - *started_at).num_seconds().max(0)
            );
        }
        TasksAuthState::AuthCancelled => {
            return "Login cancelled".to_string();
        }
        TasksAuthState::AuthTimedOut => {
            return "Login timed out. Try again.".to_string();
        }
        TasksAuthState::AuthError { message } => {
            return format!("Auth error: {message}");
        }
        TasksAuthState::Authenticated { .. } => {
            if state.sync_in_flight {
                return "Syncing...".to_string();
            }
        }
    }

    if let Some(err) = &state.last_error {
        return format!("Tasks sync error: {err}");
    }

    if state.sync_in_flight {
        return "Syncing...".to_string();
    }

    if let Some(last_sync) = state.last_sync {
        return format!("Last sync: {}", last_sync.format("%H:%M:%S"));
    }

    "Tasks not synced yet".to_string()
}
