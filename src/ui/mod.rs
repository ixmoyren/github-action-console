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
//! - `actions` — debounce and in-flight marks for anything that hits the network.
//!
//! Everything shares `AppView` and `Services` from this file.

mod actions;
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

pub(super) use std::collections::HashMap;
pub(super) use std::collections::HashSet;
pub(super) use std::sync::Arc;
pub(super) use std::time::{Duration, Instant};

pub(super) use gpui_kit::TestSupportExt as _;
pub(super) use gpui_kit::assets::IconName;
pub(super) use gpui_kit::base::input::{InputEvent, InputState};
pub(super) use gpui_kit::component::Sizable as _;
pub(super) use gpui_kit::component::button::{Button, ButtonVariants};
pub(super) use gpui_kit::component::checkbox::Checkbox;
pub(super) use gpui_kit::component::input::{Editor, EditorState, Input};
pub(super) use gpui_kit::component::notification::{Notification, NotificationList};
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
    AppProblem, AuthManager, AuthProblem, AuthState, BoardSnapshot, CreateProblem, DownloadKind,
    DownloadState, Downloads, LoadState, ManifestState, Notice, NoticeKind, PushOutcome,
    ReleaseBoard, ReleaseFacts, RepositoryList, RepositoryListState, RunActionProblem, RunDetail,
    RunProblem, SaveProblem, Status, TagProblem, Workspace, WorkspaceTab, notice_for,
};
pub(super) use crate::app_info::AppInfo;
pub(super) use crate::github::GitHubGateway;
pub(super) use crate::github::SecretToken;
pub(super) use crate::github::client::OctocrabGateway;
pub(super) use crate::github::{
    BuildArtifact, CommitSummary, Job, ReleaseAsset, Repository, RunFilter, RunStatus, Step,
    Workflow, WorkflowRun, filter_log_lines, filter_repositories, filter_runs,
};
pub(super) use crate::labels;
pub(super) use crate::release::CHANNELS;
pub(super) use crate::release_template;
pub(super) use crate::runtime::TokioRuntime;
pub(super) use crate::store::Store;
pub(super) use crate::workflow_draft::Triggers;
pub(super) use crate::workflow_draft::{
    DraftProblem, JobDraft, RUNNER_GROUPS, SELF_HOSTED_GROUP, WorkflowDraft, branch_patterns,
    runner_label, runner_versions,
};

pub(super) use actions::ActionKey;
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
    board: Arc<Mutex<ReleaseBoard>>,
    detail: Arc<Mutex<RunDetail>>,
    downloads: Arc<Mutex<Downloads>>,
    status: Arc<Mutex<Status>>,
    store: Store,
    initial_proxy: Option<String>,
    runtime: TokioRuntime,
}

/// The repository-scoped handles a load needs, in one bundle so the loader
/// signatures stay readable.
#[derive(Clone)]
pub(super) struct ScopedHandles {
    pub picker: Arc<Mutex<RepositoryList>>,
    pub workspace: Arc<Mutex<Workspace>>,
    pub board: Arc<Mutex<ReleaseBoard>>,
}

struct AppView {
    gateway: Arc<dyn GitHubGateway>,
    manager: Arc<Mutex<AuthManager>>,
    picker: Arc<Mutex<RepositoryList>>,
    workspace: Arc<Mutex<Workspace>>,
    board: Arc<Mutex<ReleaseBoard>>,
    /// The board, as the drawer renders it.
    board_view: BoardSnapshot,
    /// 建 tag 用的：仓库的分支，和这条分支上挑中的提交。
    board_branches: Vec<String>,
    /// 分支读过一次就记着；换仓库才重来。
    board_branches_loaded: bool,
    board_branch: Entity<SelectState<SearchableVec<SharedString>>>,
    board_branch_options: Vec<SharedString>,
    /// 提交是跟着分支读的：这个记着"已经为哪条分支读过提交"。
    board_branch_read: Option<String>,
    /// 要建的 tag：就是通道名（lts / latest / dogfood），默认第一个。
    board_version: Entity<SelectState<SearchableVec<SharedString>>>,
    board_commit: Entity<SelectState<SearchableVec<SharedString>>>,
    /// 读取到的提交，和下拉的选项一一对应。
    board_commits: Vec<CommitSummary>,
    /// 下拉里该有哪些选项；变了才推给 Select。
    board_commit_options: Vec<SharedString>,
    /// 三个事实：正在看的这次运行产出了什么。
    release_facts: Option<ReleaseFacts>,
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
    /// 每条工作流能不能手动跑。左列那条工作流旁边要不要给运行按钮看它。
    runnable: HashMap<u64, bool>,
    selected_workflow_id: Option<u64>,
    workflow_file: Option<String>,
    workflow_file_state: LoadState,
    runner_labels: Vec<String>,
    yaml_editor: Entity<EditorState>,
    yaml_editor_text: Option<String>,
    /// 发布模板：把仓库里那份构建脚本采用到别的仓库时，先在这里给人看和改。
    showing_template: bool,
    template_editor: Entity<EditorState>,
    template_editor_text: Option<String>,
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
    /// 过完分支过滤、排完序的那一份运行；表照着它画。
    runs_visible: Vec<WorkflowRun>,
    /// 运行列表按哪一列排：`None` 是没排（GitHub 给的顺序），`true` 是从大到小。
    run_sort: Option<(&'static str, bool)>,
    branch_subscription: Option<Subscription>,
    /// Where the runs drawer stands, and which slide is allowed to finish. A
    /// slide that has been overtaken must not take the drawer out from under the
    /// one that replaced it.
    drawer: workspace::DrawerPhase,
    drawer_kind: workspace::DrawerKind,
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
    /// 每个 job 的步骤各装在一个编辑器里。job 是网络来的，编辑器只能在有 window 的
    /// 帧里造，所以这两张表按 job id 建、按 job id 收；文本表记的是"已经喂进去的
    /// 那一份"，没喂过就是 `None`。
    job_step_editors: HashMap<u64, Entity<EditorState>>,
    job_step_texts: HashMap<u64, Option<String>>,
    logs: Option<String>,
    logs_state: LoadState,
    logs_copied: bool,
    log_input: Entity<InputState>,
    artifacts: Vec<BuildArtifact>,
    artifacts_state: LoadState,
    download_state: DownloadState,
    /// 下载完成之类的事弹一下就走：这一层是提示（toast）队列，`shell` 把它画在窗口上。
    notifications: Entity<NotificationList>,
    /// 排队等弹的提示。异步任务里没有 window，而弹提示要 window，所以先在这儿排着，
    /// 下一帧 render 交给通知层。
    pending_notifications: Vec<Notification>,
    /// 正在下的那个后台任务的把手：取消就是把它掐掉。
    download_abort: Option<tokio::task::AbortHandle>,
    /// 运行列表里展开的那条运行，以及每条运行取回来的构建产物。
    ///
    /// 只有真有可下载的构建产物时才展开（`expanded_run`），没东西可下就不占地方。
    expanded_run: Option<u64>,
    run_artifacts: HashMap<u64, RunArtifacts>,
    /// 已经点下去、还在飞的动作：没回来之前同一个动作再点不算数。
    in_flight: HashSet<actions::ActionKey>,
    /// 每个动作上一次被接受的时间，用来挡掉双击的余波。
    last_clicked: HashMap<actions::ActionKey, Instant>,
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

/// 一条运行取回来的构建产物，给运行列表里展开的那一行用。
#[derive(Clone)]
struct RunArtifacts {
    state: LoadState,
    artifacts: Vec<BuildArtifact>,
}

impl AppView {
    pub(super) fn new(services: Services, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let Services {
            gateway,
            manager,
            picker,
            workspace,
            board,
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
        let board_branch = cx.new(|cx| {
            SelectState::new(
                SearchableVec::<SharedString>::new(Vec::new()),
                None,
                window,
                cx,
            )
        });
        let board_commit = cx.new(|cx| {
            SelectState::new(
                SearchableVec::<SharedString>::new(Vec::new()),
                None,
                window,
                cx,
            )
        });
        // 版本下拉里放的就是三个通道 tag，默认第一个（lts）。
        let board_version = cx.new(|cx| {
            let tags = crate::release::CHANNELS
                .iter()
                .map(|tag| SharedString::from(*tag))
                .collect::<Vec<_>>();
            SelectState::new(
                SearchableVec::new(tags),
                Some(IndexPath::new(0)),
                window,
                cx,
            )
        });
        let yaml_editor = cx.new(|cx| yaml_editor::yaml_editor_state(window, cx));
        let template_editor = cx.new(|cx| yaml_editor::yaml_editor_state(window, cx));
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
        let notifications = cx.new(|cx| NotificationList::new(window, cx));

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
            board,
            board_view: BoardSnapshot::default(),
            board_branch,
            board_branch_options: Vec::new(),
            board_branch_read: None,
            board_branches: Vec::new(),
            board_branches_loaded: false,
            board_version,
            board_commit,
            board_commits: Vec::new(),
            board_commit_options: Vec::new(),
            release_facts: None,
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
            runnable: HashMap::new(),
            selected_workflow_id: None,
            workflow_file: None,
            workflow_file_state: LoadState::Idle,
            runner_labels: Vec::new(),
            yaml_editor,
            yaml_editor_text: None,
            showing_template: false,
            template_editor,
            template_editor_text: None,
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
            runs_visible: Vec::new(),
            run_sort: None,
            branch_subscription: None,
            drawer: workspace::DrawerPhase::Closed,
            drawer_kind: workspace::DrawerKind::Runs,
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
            job_step_editors: HashMap::new(),
            job_step_texts: HashMap::new(),
            logs: None,
            logs_state: LoadState::Idle,
            logs_copied: false,
            log_input,
            artifacts: Vec::new(),
            artifacts_state: LoadState::Idle,
            download_state: DownloadState::Idle,
            notifications,
            pending_notifications: Vec::new(),
            download_abort: None,
            expanded_run: None,
            run_artifacts: HashMap::new(),
            in_flight: HashSet::new(),
            last_clicked: HashMap::new(),
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
