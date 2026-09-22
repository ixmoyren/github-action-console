use std::sync::Arc;
use std::time::Duration;

use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Input;
use gpui_kit::component::{Root, Theme, label::Label};
use gpui_kit::*;
use tokio::sync::Mutex;

use crate::app::{
    AppProblem, AuthManager, AuthProblem, AuthState, LoadState, RepositoryList,
    RepositoryListState, Workspace, WorkspaceTab,
};
use crate::app_info::AppInfo;
use crate::github::GitHubGateway;
use crate::github::client::OctocrabGateway;
use crate::github::{
    Repository, RepositorySort, RunFilter, RunStatusFilter, Workflow, WorkflowRun,
    filter_repositories, filter_runs,
};
use crate::labels;
use crate::runtime::TokioRuntime;
use crate::store::Store;

const RUN_POLL_SECONDS: u64 = 10;

/// Everything the view shares with the rest of the app, in one bundle so the
/// constructor stays readable.
struct Services {
    gateway: Arc<dyn GitHubGateway>,
    manager: Arc<Mutex<AuthManager>>,
    picker: Arc<Mutex<RepositoryList>>,
    workspace: Arc<Mutex<Workspace>>,
    runtime: TokioRuntime,
}

struct AppView {
    info: AppInfo,
    gateway: Arc<dyn GitHubGateway>,
    manager: Arc<Mutex<AuthManager>>,
    picker: Arc<Mutex<RepositoryList>>,
    workspace: Arc<Mutex<Workspace>>,
    runtime: TokioRuntime,
    auth: AuthState,
    copied: bool,
    pat_input: Entity<InputState>,
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
}

impl AppView {
    fn new(info: AppInfo, services: Services, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let Services {
            gateway,
            manager,
            picker,
            workspace,
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

        Self {
            info,
            gateway,
            manager,
            picker,
            workspace,
            runtime,
            auth: AuthState::LoggedOut { notice: None },
            copied: false,
            pat_input,
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
        }
    }

    fn wire(&mut self, cx: &mut Context<Self>) {
        self.pat_subscription = Some(cx.subscribe(
            &self.pat_input,
            |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.submit_pat(cx);
                }
            },
        ));
    }

    // --- async plumbing -------------------------------------------------

    async fn refresh_picker(
        picker: &Arc<Mutex<RepositoryList>>,
        this: &WeakEntity<AppView>,
        cx: &mut AsyncApp,
    ) {
        let (state, repos, has_more, sort, selected) = {
            let guard = picker.lock().await;
            (
                guard.state(),
                guard.visible(),
                guard.has_more(),
                guard.sort(),
                guard.selected().map(str::to_owned),
            )
        };

        let _ = this.update(cx, |this, cx| {
            this.repo_state = state;
            this.repos = repos;
            this.repo_has_more = has_more;
            this.repo_sort = sort;
            this.selected = selected;
            cx.notify();
        });
    }

    async fn load_repositories(
        gateway: &Arc<dyn GitHubGateway>,
        manager: &Arc<Mutex<AuthManager>>,
        picker: &Arc<Mutex<RepositoryList>>,
        workspace: &Arc<Mutex<Workspace>>,
        runtime: &TokioRuntime,
        this: &WeakEntity<AppView>,
        cx: &mut AsyncApp,
    ) {
        let token = { manager.lock().await.token() };
        let Some(token) = token else {
            return;
        };

        let task = runtime.spawn({
            let picker = picker.clone();
            async move {
                picker.lock().await.restore_selection().await;
            }
        });
        let _ = task.await;

        let task = runtime.spawn({
            let picker = picker.clone();
            async move {
                picker.lock().await.reload(&token).await;
            }
        });
        let _ = task.await;

        Self::refresh_picker(picker, this, cx).await;

        // Reopen the remembered repository, if there is one.
        let remembered = { picker.lock().await.selected().map(str::to_owned) };
        if let Some(full_name) = remembered {
            let task = runtime.spawn({
                let workspace = workspace.clone();
                let full_name = full_name.clone();
                async move {
                    workspace.lock().await.enter(&full_name);
                }
            });
            let _ = task.await;

            Self::load_workspace(gateway, manager, workspace, runtime, this, cx).await;
        }
    }

    fn restore(&mut self, cx: &mut Context<Self>) {
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let picker = self.picker.clone();
        let workspace = self.workspace.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let manager = manager.clone();
                async move {
                    manager.lock().await.restore_session().await;
                }
            });
            let _ = task.await;

            let state = manager.lock().await.state().clone();
            let authenticated = matches!(state, AuthState::Authenticated { .. });
            let _ = this.update(cx, |this, cx| {
                this.auth = state;
                cx.notify();
            });

            if authenticated {
                Self::load_repositories(
                    &gateway, &manager, &picker, &workspace, &runtime, &this, cx,
                )
                .await;
            }
        })
        .detach();
    }

    fn start_login(&mut self, cx: &mut Context<Self>) {
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let picker = self.picker.clone();
        let workspace = self.workspace.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let manager = manager.clone();
                async move {
                    manager.lock().await.start_device_flow().await;
                }
            });
            let _ = task.await;

            let state = manager.lock().await.state().clone();
            let _ = this.update(cx, |this, cx| {
                this.auth = state;
                this.copied = false;
                cx.notify();
            });

            // Poll while the authorization is pending. The UI owns the cadence.
            loop {
                let interval = {
                    let guard = manager.lock().await;
                    match guard.state() {
                        AuthState::AwaitingAuthorization { start } => {
                            Duration::from_secs(start.interval_secs.max(1))
                        }
                        _ => break,
                    }
                };

                cx.background_executor().timer(interval).await;

                let task = runtime.spawn({
                    let manager = manager.clone();
                    async move {
                        manager.lock().await.poll_device_flow().await;
                    }
                });
                let _ = task.await;

                let state = manager.lock().await.state().clone();
                let finished = !matches!(state, AuthState::AwaitingAuthorization { .. });
                let authenticated = matches!(state, AuthState::Authenticated { .. });
                let _ = this.update(cx, |this, cx| {
                    this.auth = state;
                    cx.notify();
                });
                if finished {
                    if authenticated {
                        Self::load_repositories(
                            &gateway, &manager, &picker, &workspace, &runtime, &this, cx,
                        )
                        .await;
                    }
                    break;
                }
            }
        })
        .detach();
    }

    fn submit_pat(&mut self, cx: &mut Context<Self>) {
        let raw = self.pat_input.read(cx).value().to_string();
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let picker = self.picker.clone();
        let workspace = self.workspace.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let manager = manager.clone();
                async move {
                    manager.lock().await.sign_in_with_token(&raw).await;
                }
            });
            let _ = task.await;

            let state = manager.lock().await.state().clone();
            let authenticated = matches!(state, AuthState::Authenticated { .. });
            let _ = this.update(cx, |this, cx| {
                this.auth = state;
                cx.notify();
            });

            if authenticated {
                Self::load_repositories(
                    &gateway, &manager, &picker, &workspace, &runtime, &this, cx,
                )
                .await;
            }
        })
        .detach();
    }

    fn sign_out(&mut self, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
        let picker = self.picker.clone();
        let workspace = self.workspace.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let manager = manager.clone();
                async move {
                    manager.lock().await.sign_out().await;
                }
            });
            let _ = task.await;

            let task = runtime.spawn({
                let picker = picker.clone();
                async move {
                    picker.lock().await.leave_workspace().await;
                }
            });
            let _ = task.await;

            let task = runtime.spawn({
                let workspace = workspace.clone();
                async move {
                    workspace.lock().await.leave();
                }
            });
            let _ = task.await;

            let _ = this.update(cx, |this, cx| {
                this.auth = AuthState::LoggedOut { notice: None };
                this.selected = None;
                this.repos.clear();
                this.repo_state = RepositoryListState::Idle;
                this.workspace_tab = WorkspaceTab::Workflows;
                cx.notify();
            });
        })
        .detach();
    }

    fn choose_repository(&mut self, full_name: String, cx: &mut Context<Self>) {
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let picker = self.picker.clone();
        let workspace = self.workspace.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let picker = picker.clone();
                let full_name = full_name.clone();
                async move {
                    picker.lock().await.select(&full_name).await;
                }
            });
            let _ = task.await;

            let task = runtime.spawn({
                let workspace = workspace.clone();
                let full_name = full_name.clone();
                async move {
                    workspace.lock().await.enter(&full_name);
                }
            });
            let _ = task.await;

            Self::load_workspace(&gateway, &manager, &workspace, &runtime, &this, cx).await;
        })
        .detach();
    }

    fn leave_workspace(&mut self, cx: &mut Context<Self>) {
        let picker = self.picker.clone();
        let workspace = self.workspace.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let picker = picker.clone();
                async move {
                    picker.lock().await.leave_workspace().await;
                }
            });
            let _ = task.await;

            let task = runtime.spawn({
                let workspace = workspace.clone();
                async move {
                    workspace.lock().await.leave();
                }
            });
            let _ = task.await;

            let _ = this.update(cx, |this, cx| {
                this.selected = None;
                this.workflows.clear();
                this.workflows_state = LoadState::Idle;
                this.runs.clear();
                this.runs_state = LoadState::Idle;
                this.runs_has_more = false;
                this.runs_workflow_filter = None;
                this.workspace_tab = WorkspaceTab::Workflows;
                cx.notify();
            });
        })
        .detach();
    }

    // --- workspace plumbing ---------------------------------------------

    async fn refresh_workspace(
        workspace: &Arc<Mutex<Workspace>>,
        this: &WeakEntity<AppView>,
        cx: &mut AsyncApp,
    ) {
        let guard = workspace.lock().await;
        let tab = guard.tab();
        let workflows = guard.workflows().to_vec();
        let workflows_state = guard.workflows_state();
        let runs = guard.runs().to_vec();
        let runs_state = guard.runs_state();
        let runs_has_more = guard.runs_has_more();
        let selected = guard.repository().map(str::to_owned);
        let status = guard.run_filter().status;
        let workflow_filter = guard.run_filter().workflow_id;
        drop(guard);

        let _ = this.update(cx, |this, cx| {
            this.workspace_tab = tab;
            this.workflows = workflows;
            this.workflows_state = workflows_state;
            this.runs = runs;
            this.runs_state = runs_state;
            this.runs_has_more = runs_has_more;
            this.selected = selected;
            this.runs_status_filter = status;
            this.runs_workflow_filter = workflow_filter;
            this.poll_runs_if_needed(cx);
            cx.notify();
        });
    }

    async fn load_workspace(
        gateway: &Arc<dyn GitHubGateway>,
        manager: &Arc<Mutex<AuthManager>>,
        workspace: &Arc<Mutex<Workspace>>,
        runtime: &TokioRuntime,
        this: &WeakEntity<AppView>,
        cx: &mut AsyncApp,
    ) {
        let token = { manager.lock().await.token() };
        let Some(token) = token else {
            return;
        };

        let task = runtime.spawn({
            let gateway = gateway.clone();
            let workspace = workspace.clone();
            let token = token.clone();
            async move {
                workspace
                    .lock()
                    .await
                    .load_workflows(&*gateway, &token)
                    .await;
            }
        });
        let _ = task.await;

        let task = runtime.spawn({
            let gateway = gateway.clone();
            let workspace = workspace.clone();
            let token = token.clone();
            async move {
                workspace.lock().await.reload_runs(&*gateway, &token).await;
            }
        });
        let _ = task.await;

        Self::refresh_workspace(workspace, this, cx).await;
    }

    fn set_workspace_tab(&mut self, tab: WorkspaceTab, cx: &mut Context<Self>) {
        self.workspace_tab = tab;
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let workspace = self.workspace.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let workspace = workspace.clone();
                async move {
                    workspace.lock().await.set_tab(tab);
                }
            });
            let _ = task.await;

            if tab == WorkspaceTab::Runs {
                let token = { manager.lock().await.token() };
                if let Some(token) = token {
                    let task = runtime.spawn({
                        let gateway = gateway.clone();
                        let workspace = workspace.clone();
                        async move {
                            workspace.lock().await.reload_runs(&*gateway, &token).await;
                        }
                    });
                    let _ = task.await;
                }
            }

            Self::refresh_workspace(&workspace, &this, cx).await;
        })
        .detach();
    }

    fn load_more_runs(&mut self, cx: &mut Context<Self>) {
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let workspace = self.workspace.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };
            let task = runtime.spawn({
                let gateway = gateway.clone();
                let workspace = workspace.clone();
                async move {
                    workspace
                        .lock()
                        .await
                        .load_more_runs(&*gateway, &token)
                        .await;
                }
            });
            let _ = task.await;
            Self::refresh_workspace(&workspace, &this, cx).await;
        })
        .detach();
    }

    fn set_workflow_filter(&mut self, workflow_id: Option<u64>, cx: &mut Context<Self>) {
        self.runs_workflow_filter = workflow_id;
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let workspace = self.workspace.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };
            let task = runtime.spawn({
                let gateway = gateway.clone();
                let workspace = workspace.clone();
                async move {
                    let mut guard = workspace.lock().await;
                    guard.set_workflow_filter(workflow_id);
                    guard.reload_runs(&*gateway, &token).await;
                }
            });
            let _ = task.await;
            Self::refresh_workspace(&workspace, &this, cx).await;
        })
        .detach();
    }

    /// Poll while runs are moving. The loop re-checks the workspace each round,
    /// so switching tabs or leaving the repository stops it (ticket 05).
    fn poll_runs_if_needed(&mut self, cx: &mut Context<Self>) {
        if self.polling {
            return;
        }
        let needed = self.selected.is_some()
            && self.workspace_tab == WorkspaceTab::Runs
            && self.runs.iter().any(|run| run.status.is_running());
        if !needed {
            return;
        }

        self.polling = true;
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let workspace = self.workspace.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            loop {
                let should_poll = { workspace.lock().await.should_poll_runs() };
                if !should_poll {
                    break;
                }

                cx.background_executor()
                    .timer(Duration::from_secs(RUN_POLL_SECONDS))
                    .await;

                let token = { manager.lock().await.token() };
                let Some(token) = token else {
                    break;
                };
                let task = runtime.spawn({
                    let gateway = gateway.clone();
                    let workspace = workspace.clone();
                    async move {
                        workspace.lock().await.reload_runs(&*gateway, &token).await;
                    }
                });
                let _ = task.await;

                Self::refresh_workspace(&workspace, &this, cx).await;
            }

            let _ = this.update(cx, |this, cx| {
                this.polling = false;
                cx.notify();
            });
        })
        .detach();
    }

    fn load_more(&mut self, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
        let picker = self.picker.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };

            let task = runtime.spawn({
                let picker = picker.clone();
                async move {
                    picker.lock().await.load_more(&token).await;
                }
            });
            let _ = task.await;

            Self::refresh_picker(&picker, &this, cx).await;
        })
        .detach();
    }

    fn change_sort(&mut self, sort: RepositorySort, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
        let picker = self.picker.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };

            let task = runtime.spawn({
                let picker = picker.clone();
                async move {
                    let mut guard = picker.lock().await;
                    guard.set_sort(sort);
                    guard.reload(&token).await;
                }
            });
            let _ = task.await;

            Self::refresh_picker(&picker, &this, cx).await;
        })
        .detach();
    }
}

fn notice_text(problem: AuthProblem) -> &'static str {
    match problem {
        AuthProblem::DeviceFlowUnavailable => labels::NOTICE_DEVICE_FLOW_UNAVAILABLE,
        AuthProblem::Expired => labels::NOTICE_EXPIRED,
        AuthProblem::Denied => labels::NOTICE_DENIED,
        AuthProblem::InvalidCredentials => labels::NOTICE_INVALID_CREDENTIALS,
        AuthProblem::MissingScopes => labels::NOTICE_MISSING_SCOPES,
        AuthProblem::Network => labels::NOTICE_NETWORK,
        AuthProblem::Unexpected => labels::NOTICE_UNEXPECTED,
    }
}

fn problem_text(problem: AppProblem) -> &'static str {
    match problem {
        AppProblem::Forbidden => labels::NOTICE_MISSING_SCOPES,
        AppProblem::RateLimited => labels::PROBLEM_RATE_LIMITED,
        AppProblem::NotFound => labels::PROBLEM_NOT_FOUND,
        AppProblem::Network => labels::NOTICE_NETWORK,
        AppProblem::Unexpected => labels::NOTICE_UNEXPECTED,
    }
}

fn info_row(label: &'static str, value: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(Label::new(label).text_sm())
        .child(Label::new(value.to_owned()))
}

impl AppView {
    fn header(&self) -> impl IntoElement {
        let packaging_config = self.info.packaging_config().unwrap_or(labels::UNSPECIFIED);

        div()
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .child(Label::new(labels::APP_TITLE))
            .child(info_row(labels::LABEL_VERSION, self.info.version()))
            .child(info_row(
                labels::LABEL_BUILD_TARGET,
                self.info.build_target(),
            ))
            .child(info_row(labels::LABEL_PACKAGING_CONFIG, packaging_config))
    }

    fn account_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        match &self.auth {
            AuthState::Authenticated { account } => Some(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .p_3()
                    .child(Label::new(format!(
                        "{}：{}",
                        labels::LOGIN_LOGGED_IN_AS,
                        account.login
                    )))
                    .child(
                        Button::new("sign-out")
                            .label(labels::LOGIN_SIGN_OUT)
                            .on_click(cx.listener(|this, _, _, cx| this.sign_out(cx))),
                    )
                    .into_any_element(),
            ),
            _ => None,
        }
    }

    fn login_page(&self, state: AuthState, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = div().flex().flex_col().items_center().gap_3();

        match state {
            AuthState::LoggedOut { notice } => {
                panel = panel
                    .children(notice.map(|problem| Label::new(notice_text(problem)).text_sm()))
                    .child(
                        Button::new("start-login")
                            .label(labels::LOGIN_START)
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| this.start_login(cx))),
                    );
            }
            AuthState::StartingDeviceFlow => {
                panel = panel.child(Label::new(labels::LOGIN_STARTING));
            }
            AuthState::AwaitingAuthorization { start } => {
                let code = start.user_code.clone();
                let uri = start.verification_uri.clone();
                panel = panel
                    .child(Label::new(labels::LOGIN_INSTRUCTION))
                    .child(Label::new(start.verification_uri.clone()))
                    .child(Label::new(format!(
                        "{}：{}",
                        labels::LOGIN_USER_CODE,
                        start.user_code
                    )))
                    .child(Label::new(format!(
                        "{}：{}",
                        labels::LOGIN_EXPIRES_IN,
                        start.expires_in_secs
                    )))
                    .child(Label::new(labels::LOGIN_WAITING))
                    .child(Label::new(if self.copied {
                        labels::LOGIN_COPIED
                    } else {
                        ""
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_3()
                            .child(Button::new("copy-code").label(labels::LOGIN_COPY).on_click(
                                cx.listener(move |this, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
                                    this.copied = true;
                                    cx.notify();
                                }),
                            ))
                            .child(
                                Button::new("open-verification")
                                    .label(labels::LOGIN_OPEN_BROWSER)
                                    .on_click(move |_, _, _| {
                                        let _ = open::that(uri.clone());
                                    }),
                            ),
                    );
            }
            AuthState::ValidatingCredentials => {
                panel = panel.child(Label::new(labels::LOGIN_VALIDATING));
            }
            AuthState::Authenticated { .. } => {}
        }

        panel
            .child(Label::new(labels::LOGIN_PAT_TITLE))
            .child(Input::new(&self.pat_input))
            .child(
                Button::new("submit-pat")
                    .label(labels::LOGIN_PAT_SUBMIT)
                    .on_click(cx.listener(|this, _, _, cx| this.submit_pat(cx))),
            )
            .into_any_element()
    }

    fn repository_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let query = self.search_input.read(cx).value().to_string();
        let visible = filter_repositories(&self.repos, &query);

        let items = visible
            .iter()
            .map(|repository| {
                let full_name = repository.full_name.clone();
                let visibility = if repository.is_private {
                    labels::REPOSITORIES_PRIVATE
                } else {
                    labels::REPOSITORIES_PUBLIC
                };
                Button::new(SharedString::from(format!("repo-{}", repository.full_name)))
                    .label(format!("{}（{}）", repository.full_name, visibility))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.choose_repository(full_name.clone(), cx)
                    }))
            })
            .collect::<Vec<_>>();

        let mut list = div().flex().flex_col().gap_1().children(items);

        match self.repo_state {
            RepositoryListState::Loading if self.repos.is_empty() => {
                list = list.child(Label::new(labels::REPOSITORIES_LOADING));
            }
            RepositoryListState::Failed(problem) => {
                list = list.child(Label::new(problem_text(problem)).text_sm());
            }
            RepositoryListState::Loaded if visible.is_empty() => {
                list = list.child(Label::new(labels::REPOSITORIES_EMPTY));
            }
            _ => {}
        }

        if self.repo_has_more {
            list = list.child(
                Button::new("load-more")
                    .label(labels::REPOSITORIES_LOAD_MORE)
                    .on_click(cx.listener(|this, _, _, cx| this.load_more(cx))),
            );
        }

        let updated_selected = self.repo_sort == RepositorySort::Updated;
        let mut sort_updated = Button::new("sort-updated").label(labels::REPOSITORIES_SORT_UPDATED);
        if updated_selected {
            sort_updated = sort_updated.primary();
        }
        let mut sort_pushed = Button::new("sort-pushed").label(labels::REPOSITORIES_SORT_PUSHED);
        if !updated_selected {
            sort_pushed = sort_pushed.primary();
        }

        let sort_row = div()
            .flex()
            .flex_row()
            .gap_2()
            .child(sort_updated.on_click(
                cx.listener(|this, _, _, cx| this.change_sort(RepositorySort::Updated, cx)),
            ))
            .child(sort_pushed.on_click(
                cx.listener(|this, _, _, cx| this.change_sort(RepositorySort::Pushed, cx)),
            ));

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .child(Label::new(labels::REPOSITORIES_TITLE))
            .child(Input::new(&self.search_input))
            .child(sort_row)
            .child(list)
            .into_any_element()
    }

    fn workspace_shell(&self, full_name: &str, cx: &mut Context<Self>) -> AnyElement {
        let mut workflows_tab = Button::new("tab-workflows").label(labels::WORKSPACE_WORKFLOWS);
        if self.workspace_tab == WorkspaceTab::Workflows {
            workflows_tab = workflows_tab.primary();
        }
        let mut runs_tab = Button::new("tab-runs").label(labels::WORKSPACE_RUNS);
        if self.workspace_tab == WorkspaceTab::Runs {
            runs_tab = runs_tab.primary();
        }

        let tabs = div()
            .flex()
            .flex_row()
            .gap_2()
            .child(workflows_tab.on_click(cx.listener(|this, _, _, cx| {
                this.set_workspace_tab(WorkspaceTab::Workflows, cx);
            })))
            .child(runs_tab.on_click(cx.listener(|this, _, _, cx| {
                this.set_workspace_tab(WorkspaceTab::Runs, cx);
            })));

        let content = match self.workspace_tab {
            WorkspaceTab::Workflows => self.workflows_panel_ui(),
            WorkspaceTab::Runs => self.runs_panel_ui(cx),
        };

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .child(
                Button::new("back-to-repos")
                    .label(labels::WORKSPACE_BACK)
                    .on_click(cx.listener(|this, _, _, cx| this.leave_workspace(cx))),
            )
            .child(Label::new(full_name.to_owned()))
            .child(tabs)
            .child(content)
            .into_any_element()
    }

    fn workflows_panel_ui(&self) -> AnyElement {
        let mut panel = div().flex().flex_col().gap_2().children(
            self.workflows
                .iter()
                .map(|workflow| Label::new(workflow.name.clone())),
        );

        match self.workflows_state {
            LoadState::Loading => panel = panel.child(Label::new(labels::WORKFLOWS_LOADING)),
            LoadState::Failed(problem) => {
                panel = panel.child(Label::new(problem_text(problem)).text_sm())
            }
            LoadState::Loaded if self.workflows.is_empty() => {
                panel = panel.child(Label::new(labels::WORKFLOWS_EMPTY))
            }
            _ => {}
        }

        panel.into_any_element()
    }

    fn runs_panel_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        let status_button = |id: &'static str, label: &'static str, active: bool| {
            let mut button = Button::new(id).label(label);
            if active {
                button = button.primary();
            }
            button
        };

        let status_row = div()
            .flex()
            .flex_row()
            .gap_2()
            .child(
                status_button(
                    "runs-status-all",
                    labels::RUNS_FILTER_ALL,
                    self.runs_status_filter == RunStatusFilter::All,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.runs_status_filter = RunStatusFilter::All;
                    cx.notify();
                })),
            )
            .child(
                status_button(
                    "runs-status-running",
                    labels::RUNS_FILTER_RUNNING,
                    self.runs_status_filter == RunStatusFilter::Running,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.runs_status_filter = RunStatusFilter::Running;
                    cx.notify();
                })),
            )
            .child(
                status_button(
                    "runs-status-completed",
                    labels::RUNS_FILTER_COMPLETED,
                    self.runs_status_filter == RunStatusFilter::Completed,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.runs_status_filter = RunStatusFilter::Completed;
                    cx.notify();
                })),
            );

        let mut workflow_buttons = vec![
            status_button(
                "workflow-all",
                labels::RUNS_FILTER_WORKFLOW_ALL,
                self.runs_workflow_filter.is_none(),
            )
            .on_click(cx.listener(|this, _, _, cx| this.set_workflow_filter(None, cx)))
            .into_any_element(),
        ];
        for workflow in &self.workflows {
            let id = workflow.id;
            let mut button = Button::new(SharedString::from(format!("workflow-{id}")))
                .label(workflow.name.clone());
            if self.runs_workflow_filter == Some(id) {
                button = button.primary();
            }
            workflow_buttons.push(
                button
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.set_workflow_filter(Some(id), cx)),
                    )
                    .into_any_element(),
            );
        }
        let workflow_row = div().flex().flex_row().gap_2().children(workflow_buttons);

        let branch_text = self.branch_input.read(cx).value().to_string();
        let branch_filter = if branch_text.trim().is_empty() {
            None
        } else {
            Some(branch_text.trim().to_owned())
        };
        let filter = RunFilter {
            status: self.runs_status_filter,
            branch: branch_filter,
            workflow_id: self.runs_workflow_filter,
        };
        let visible = filter_runs(&self.runs, &filter);

        let rows = visible
            .iter()
            .map(|run| {
                Label::new(format!(
                    "{}｜{}｜{}｜{}｜{}｜{}｜{}",
                    run.status.label(),
                    run.name,
                    run.conclusion
                        .clone()
                        .unwrap_or_else(|| labels::VALUE_MISSING.to_owned()),
                    run.branch
                        .clone()
                        .unwrap_or_else(|| labels::VALUE_MISSING.to_owned()),
                    run.event,
                    run.actor
                        .clone()
                        .unwrap_or_else(|| labels::VALUE_MISSING.to_owned()),
                    run.created_at
                        .clone()
                        .unwrap_or_else(|| labels::VALUE_MISSING.to_owned()),
                ))
            })
            .collect::<Vec<_>>();

        let mut list = div().flex().flex_col().gap_1().children(rows);
        match self.runs_state {
            LoadState::Loading if self.runs.is_empty() => {
                list = list.child(Label::new(labels::RUNS_LOADING));
            }
            LoadState::Failed(problem) => {
                list = list.child(Label::new(problem_text(problem)).text_sm());
            }
            LoadState::Loaded if visible.is_empty() => {
                list = list.child(Label::new(labels::RUNS_EMPTY));
            }
            _ => {}
        }
        if self.runs_has_more {
            list = list.child(
                Button::new("runs-load-more")
                    .label(labels::RUNS_LOAD_MORE)
                    .on_click(cx.listener(|this, _, _, cx| this.load_more_runs(cx))),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(status_row)
            .child(workflow_row)
            .child(Input::new(&self.branch_input))
            .child(list)
            .into_any_element()
    }
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header = self.header();
        let account_row = self.account_row(cx);

        let body = match self.auth.clone() {
            AuthState::Authenticated { .. } => match self.selected.clone() {
                Some(full_name) => self.workspace_shell(&full_name, cx),
                None => self.repository_picker(cx),
            },
            other => self.login_page(other, cx),
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .children(account_row)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(body),
            )
    }
}

fn default_store_path() -> std::io::Result<std::path::PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(std::path::PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(|home| std::path::PathBuf::from(home).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| std::path::PathBuf::from(home).join(".local/share"))
            })
    };

    let dir = base
        .unwrap_or_else(std::env::temp_dir)
        .join("github-action-console");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("console.sqlite"))
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let info = AppInfo::from_build();
    let runtime = TokioRuntime::new()?;
    let store = runtime.block_on(Store::open(default_store_path()?))?;
    let gateway: Arc<dyn GitHubGateway> = Arc::new(OctocrabGateway::new(
        option_env!("GITHUB_CLIENT_ID").map(str::to_owned),
    ));
    let manager = Arc::new(Mutex::new(AuthManager::new(gateway.clone(), store.clone())));
    let picker = Arc::new(Mutex::new(RepositoryList::new(gateway.clone(), store)));
    let workspace = Arc::new(Mutex::new(Workspace::new()));

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        let info = info.clone();
        let gateway = gateway.clone();
        let manager = manager.clone();
        let picker = picker.clone();
        let workspace = workspace.clone();
        let runtime = runtime.clone();

        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some(labels::APP_TITLE.into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    Theme::sync_system_appearance(Some(window), cx);
                    let services = Services {
                        gateway,
                        manager,
                        picker,
                        workspace,
                        runtime,
                    };
                    let view = cx.new(|cx| AppView::new(info, services, window, cx));
                    view.update(cx, |this, cx| {
                        this.wire(cx);
                        this.restore(cx);
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("failed to open window");
        })
        .detach();
    });

    Ok(())
}
