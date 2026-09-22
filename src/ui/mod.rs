//! The desktop UI, split by responsibility:
//!
//! - `launch` — composition root: opens the window and wires the services.
//! - `shell` — the app frame: header, info rows, shared copy, and `render`.
//! - `session` — login and session restore (device flow, PAT, sign out).
//! - `repositories` — the repository picker.
//! - `workspace` — a repository's workflows, runs, filters, and polling.
//! - `run_detail` — a run's jobs, steps, and logs.
//! - `downloads` — run log archives and build artifacts.
//! - `status` — the persistent status bar and transient notices.
//!
//! Everything shares `AppView` and `Services` from this file.

mod downloads;
mod launch;
mod repositories;
mod run_detail;
mod session;
mod settings;
mod shell;
mod status;
mod workspace;

pub use launch::run;

pub(super) use std::sync::Arc;
pub(super) use std::time::Duration;

pub(super) use gpui_kit::assets::IconName;
pub(super) use gpui_kit::base::input::{InputEvent, InputState};
pub(super) use gpui_kit::component::button::{Button, ButtonVariants};
pub(super) use gpui_kit::component::input::Input;
pub(super) use gpui_kit::component::scroll::ScrollableElement as _;
pub(super) use gpui_kit::component::{Root, Theme, label::Label};
pub(super) use gpui_kit::*;
pub(super) use tokio::sync::Mutex;
pub(super) use tracing::{info, warn};

pub(super) use crate::app::{
    AppProblem, AuthManager, AuthProblem, AuthState, DownloadState, Downloads, LoadState, Notice,
    RepositoryList, RepositoryListState, RunDetail, Status, Workspace, WorkspaceTab,
};
pub(super) use crate::app_info::AppInfo;
pub(super) use crate::github::GitHubGateway;
pub(super) use crate::github::client::OctocrabGateway;
pub(super) use crate::github::{
    BuildArtifact, Job, Repository, RepositorySort, RunFilter, RunStatusFilter, Workflow,
    WorkflowRun, filter_log_lines, filter_repositories, filter_runs,
};
pub(super) use crate::labels;
pub(super) use crate::runtime::TokioRuntime;
pub(super) use crate::store::Store;

pub(crate) use shell::{notice_text, pickable, problem_text, reset_pickable_ids};

const RUN_POLL_SECONDS: u64 = 10;

/// Which screen the signed-out flow is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum LoginStep {
    /// The two entry buttons.
    #[default]
    Home,
    /// The Personal Access Token entry screen.
    PatEntry,
    /// The OAuth App client-id entry screen.
    ClientIdEntry,
    /// The device-flow screen: waiting for GitHub, or showing why it failed.
    DeviceFlow,
}

/// Everything the view shares with the rest of the app, in one bundle so the
/// constructor stays readable.
struct Services {
    gateway: Arc<dyn GitHubGateway>,
    manager: Arc<Mutex<AuthManager>>,
    picker: Arc<Mutex<RepositoryList>>,
    workspace: Arc<Mutex<Workspace>>,
    detail: Arc<Mutex<RunDetail>>,
    downloads: Arc<Mutex<Downloads>>,
    status: Arc<Mutex<Status>>,
    store: Store,
    initial_proxy: Option<String>,
    runtime: TokioRuntime,
}

struct AppView {
    gateway: Arc<dyn GitHubGateway>,
    manager: Arc<Mutex<AuthManager>>,
    picker: Arc<Mutex<RepositoryList>>,
    workspace: Arc<Mutex<Workspace>>,
    runtime: TokioRuntime,
    auth: AuthState,
    copied: bool,
    window_title: Option<String>,
    resume_prompt: bool,
    proxy_input: Entity<InputState>,
    store: Store,
    flow_remaining_secs: Option<u64>,
    login_step: LoginStep,
    pat_input: Entity<InputState>,
    client_id_input: Entity<InputState>,
    search_input: Entity<InputState>,
    branch_input: Entity<InputState>,
    pat_subscription: Option<Subscription>,
    repos: Vec<Repository>,
    repo_state: RepositoryListState,
    repo_has_more: bool,
    repo_sort: RepositorySort,
    selected: Option<String>,
    workflows: Vec<Workflow>,
    workflows_state: LoadState,
    runs: Vec<WorkflowRun>,
    runs_state: LoadState,
    runs_has_more: bool,
    runs_status_filter: RunStatusFilter,
    runs_workflow_filter: Option<u64>,
    workspace_tab: WorkspaceTab,
    polling: bool,
    detail: Arc<Mutex<RunDetail>>,
    downloads: Arc<Mutex<Downloads>>,
    open_run: Option<u64>,
    run_html_url: Option<String>,
    jobs: Vec<Job>,
    jobs_state: LoadState,
    selected_job: Option<u64>,
    logs: Option<String>,
    logs_state: LoadState,
    logs_copied: bool,
    log_input: Entity<InputState>,
    artifacts: Vec<BuildArtifact>,
    artifacts_state: LoadState,
    download_state: DownloadState,
    status: Arc<Mutex<Status>>,
    status_account: Option<String>,
    status_remaining: Option<u64>,
    status_reset_at: Option<String>,
    notices: Vec<Notice>,
}

impl AppView {
    pub(super) fn new(services: Services, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let Services {
            gateway,
            manager,
            picker,
            workspace,
            detail,
            downloads,
            status,
            store,
            initial_proxy,
            runtime,
        } = services;
        let pat_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(labels::LOGIN_PAT_PLACEHOLDER, window, cx);
            state
        });
        let search_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(labels::REPOSITORIES_SEARCH_PLACEHOLDER, window, cx);
            state
        });
        let branch_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(labels::RUNS_BRANCH_PLACEHOLDER, window, cx);
            state
        });
        let proxy_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(labels::SETTINGS_PROXY_PLACEHOLDER, window, cx);
            if let Some(proxy) = initial_proxy.clone() {
                state.set_value(proxy, window, cx);
            }
            state
        });
        let client_id_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(labels::LOGIN_CLIENT_ID_PLACEHOLDER, window, cx);
            state
        });
        let log_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(labels::LOGS_SEARCH_PLACEHOLDER, window, cx);
            state
        });

        Self {
            gateway,
            manager,
            picker,
            workspace,
            runtime,
            auth: AuthState::LoggedOut { notice: None },
            copied: false,
            window_title: None,
            resume_prompt: false,
            proxy_input,
            store,
            flow_remaining_secs: None,
            login_step: LoginStep::Home,
            pat_input,
            client_id_input,
            search_input,
            branch_input,
            pat_subscription: None,
            repos: Vec::new(),
            repo_state: RepositoryListState::Idle,
            repo_has_more: false,
            repo_sort: RepositorySort::Updated,
            selected: None,
            workflows: Vec::new(),
            workflows_state: LoadState::Idle,
            runs: Vec::new(),
            runs_state: LoadState::Idle,
            runs_has_more: false,
            runs_status_filter: RunStatusFilter::All,
            runs_workflow_filter: None,
            workspace_tab: WorkspaceTab::Workflows,
            polling: false,
            detail,
            downloads,
            open_run: None,
            run_html_url: None,
            jobs: Vec::new(),
            jobs_state: LoadState::Idle,
            selected_job: None,
            logs: None,
            logs_state: LoadState::Idle,
            logs_copied: false,
            log_input,
            artifacts: Vec::new(),
            artifacts_state: LoadState::Idle,
            download_state: DownloadState::Idle,
            status,
            status_account: None,
            status_remaining: None,
            status_reset_at: None,
            notices: Vec::new(),
        }
    }
}
