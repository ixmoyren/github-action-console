//! The desktop UI, split by responsibility:
//!
//! - `launch` — composition root: opens the window and wires the services.
//! - `shell` — the app frame: header, info rows, shared copy, and `render`.
//! - `session` — login and session restore (device flow, PAT, sign out).
//! - `repositories` — the repository picker.
//! - `workspace` — a repository's workflows, runs, filters, and polling.
//! - `yaml_editor` — the workflow-file editor and its YAML highlighter.
//! - `run_detail` — a run's jobs, steps, and logs.
//! - `downloads` — run log archives and build artifacts.
//! - `status` — the persistent status bar and transient notices.
//!
//! Everything shares `AppView` and `Services` from this file.

mod downloads;
mod launch;
mod repositories;
mod run_detail;
mod runs;
mod session;
mod settings;
mod shell;
mod status;
mod workspace;
mod yaml_editor;

pub use launch::run;

pub(super) use std::sync::Arc;
pub(super) use std::time::Duration;

pub(super) use gpui_kit::TestSupportExt as _;
pub(super) use gpui_kit::assets::IconName;
pub(super) use gpui_kit::base::input::{InputEvent, InputState};
pub(super) use gpui_kit::component::button::{Button, ButtonVariants};
pub(super) use gpui_kit::component::checkbox::Checkbox;
pub(super) use gpui_kit::component::input::{Editor, EditorState, Input};
pub(super) use gpui_kit::component::scroll::ScrollableElement as _;
pub(super) use gpui_kit::component::select::{SearchableVec, Select, SelectState};
pub(super) use gpui_kit::component::table::{Column, TableState};
pub(super) use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IndexPath, Root, Theme, label::Label,
};
pub(super) use gpui_kit::prelude::FluentBuilder as _;
pub(super) use gpui_kit::*;
pub(super) use tokio::sync::Mutex;
pub(super) use tracing::{info, warn};

pub(super) use crate::app::{
    AppProblem, AuthManager, AuthProblem, AuthState, CreateProblem, DownloadState, Downloads,
    LoadState, Notice, NoticeKind, PushOutcome, RepositoryList, RepositoryListState, RunDetail,
    RunProblem, SaveProblem, Status, Workspace, WorkspaceTab, notice_for,
};
pub(super) use crate::app_info::AppInfo;
pub(super) use crate::github::GitHubGateway;
pub(super) use crate::github::client::OctocrabGateway;
pub(super) use crate::github::{
    BuildArtifact, CommitSummary, Job, Repository, RunFilter, Workflow, WorkflowRun,
    filter_log_lines, filter_repositories, filter_runs,
};
pub(super) use crate::labels;
pub(super) use crate::runtime::TokioRuntime;
pub(super) use crate::store::Store;
pub(super) use crate::workflow_draft::Triggers;
pub(super) use crate::workflow_draft::{
    DraftProblem, JobDraft, RUNNER_GROUPS, SELF_HOSTED_GROUP, WorkflowDraft, branch_patterns,
    runner_label, runner_versions,
};

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
    search_subscription: Option<Subscription>,
    repo_table: Entity<TableState<repositories::RepositoryTableDelegate>>,
    repos: Vec<Repository>,
    repo_state: RepositoryListState,
    repo_has_more: bool,
    selected: Option<String>,
    workflows: Vec<Workflow>,
    workflows_state: LoadState,
    selected_workflow_id: Option<u64>,
    workflow_file: Option<String>,
    workflow_file_state: LoadState,
    runner_labels: Vec<String>,
    yaml_editor: Entity<EditorState>,
    yaml_editor_text: Option<String>,
    creating_workflow: bool,
    previewing_draft: bool,
    draft_file_name: Entity<InputState>,
    draft_name: Entity<InputState>,
    draft_container: Entity<InputState>,
    draft_runner_os: Entity<SelectState<SearchableVec<SharedString>>>,
    draft_runner_version: Entity<SelectState<SearchableVec<SharedString>>>,
    /// The choices the two runner dropdowns were last filled with, so they are
    /// only rebuilt when GitHub's runner labels change or the system does.
    draft_os_options: Vec<SharedString>,
    draft_version_options: Vec<SharedString>,
    draft_manual: bool,
    draft_push: bool,
    draft_push_branches: Entity<InputState>,
    draft_pull_request: bool,
    draft_pull_request_branches: Entity<InputState>,
    draft_schedule: bool,
    draft_cron: Entity<InputState>,
    draft_jobs: Vec<DraftJobRow>,
    preview_editor: Entity<EditorState>,
    preview_editor_text: Option<String>,
    runs: Vec<WorkflowRun>,
    runs_state: LoadState,
    runs_has_more: bool,
    run_table: Entity<TableState<runs::RunTableDelegate>>,
    branch_subscription: Option<Subscription>,
    /// Where the runs drawer stands, and which slide is allowed to finish. A
    /// slide that has been overtaken must not take the drawer out from under the
    /// one that replaced it.
    drawer: workspace::DrawerPhase,
    drawer_generation: u64,
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

/// One job of the new-workflow form: an input per field it asks for.
struct DraftJobRow {
    id: Entity<InputState>,
    name: Entity<InputState>,
    command: Entity<InputState>,
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
        let yaml_editor = cx.new(|cx| yaml_editor::yaml_editor_state(window, cx));
        let draft_file_name = cx.new(|cx| draft_input(window, cx, labels::WORKFLOW_NEW_FILE_HINT));
        let draft_name = cx.new(|cx| draft_input(window, cx, labels::WORKFLOW_NEW_NAME_HINT));
        let draft_container =
            cx.new(|cx| draft_input(window, cx, labels::WORKFLOW_NEW_CONTAINER_HINT));
        let draft_cron = cx.new(|cx| draft_input(window, cx, labels::WORKFLOW_NEW_CRON_HINT));
        let draft_push_branches =
            cx.new(|cx| draft_input(window, cx, labels::WORKFLOW_NEW_BRANCHES_HINT));
        let draft_pull_request_branches =
            cx.new(|cx| draft_input(window, cx, labels::WORKFLOW_NEW_BRANCHES_HINT));
        let draft_runner_os = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(runner_groups(&[])),
                Some(IndexPath::new(0)),
                window,
                cx,
            )
        });
        let draft_runner_version = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(runner_versions_ui(RUNNER_GROUPS[0])),
                Some(IndexPath::new(0)),
                window,
                cx,
            )
        });
        let draft_job_id = cx.new(|cx| draft_input(window, cx, labels::WORKFLOW_NEW_JOB_ID_HINT));
        let draft_job_name =
            cx.new(|cx| draft_input(window, cx, labels::WORKFLOW_NEW_JOB_NAME_HINT));
        let draft_job_command =
            cx.new(|cx| draft_input(window, cx, labels::WORKFLOW_NEW_JOB_COMMAND_HINT));
        let draft_jobs = vec![DraftJobRow {
            id: draft_job_id,
            name: draft_job_name,
            command: draft_job_command,
        }];
        let preview_editor = cx.new(|cx| yaml_editor::yaml_editor_state(window, cx));

        let run_view = cx.weak_entity();
        let run_table = cx.new(|cx| {
            TableState::new(runs::RunTableDelegate::new(run_view), window, cx)
                .col_resizable(true)
                .sortable(true)
        });

        let repo_view = cx.weak_entity();
        let repo_table = cx.new(move |cx| {
            TableState::new(
                repositories::RepositoryTableDelegate::new(repo_view),
                window,
                cx,
            )
            .row_selectable(true)
            .col_resizable(true)
            .sortable(true)
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
            search_subscription: None,
            repo_table,
            repos: Vec::new(),
            repo_state: RepositoryListState::Idle,
            repo_has_more: false,
            selected: None,
            workflows: Vec::new(),
            workflows_state: LoadState::Idle,
            selected_workflow_id: None,
            workflow_file: None,
            workflow_file_state: LoadState::Idle,
            runner_labels: Vec::new(),
            yaml_editor,
            yaml_editor_text: None,
            creating_workflow: false,
            previewing_draft: false,
            draft_file_name,
            draft_name,
            draft_container,
            draft_runner_os,
            draft_runner_version,
            draft_os_options: runner_groups(&[]),
            draft_version_options: runner_versions_ui(RUNNER_GROUPS[0]),
            draft_manual: true,
            draft_push: false,
            draft_push_branches,
            draft_pull_request: false,
            draft_pull_request_branches,
            draft_schedule: false,
            draft_cron,
            draft_jobs,
            preview_editor,
            preview_editor_text: None,
            runs: Vec::new(),
            runs_state: LoadState::Idle,
            runs_has_more: false,
            run_table,
            branch_subscription: None,
            drawer: workspace::DrawerPhase::Closed,
            drawer_generation: 0,
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

/// One field of the new-workflow form.
fn draft_input(
    window: &mut Window,
    cx: &mut Context<InputState>,
    placeholder: &'static str,
) -> InputState {
    let mut state = InputState::new(window, cx);
    state.set_placeholder(placeholder, window, cx);
    state
}

/// The operating systems to choose from: the hosted ones, plus the repository's
/// own runners when GitHub reported any.
fn runner_groups(self_hosted: &[String]) -> Vec<SharedString> {
    let mut groups: Vec<SharedString> = RUNNER_GROUPS
        .iter()
        .map(|group| SharedString::from(*group))
        .collect();
    if !self_hosted.is_empty() {
        groups.push(SharedString::from(SELF_HOSTED_GROUP));
    }
    groups
}

/// The versions the chosen system offers. A self-hosted system's "versions" are
/// the labels its runners answer to.
fn runner_versions_ui(group: &str) -> Vec<SharedString> {
    runner_versions(group)
        .iter()
        .map(|version| SharedString::from(*version))
        .collect()
}
