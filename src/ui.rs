use std::sync::Arc;
use std::time::Duration;

use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Input;
use gpui_kit::component::{Root, Theme, label::Label};
use gpui_kit::*;
use tokio::sync::Mutex;

use crate::app::{
    AppProblem, AuthManager, AuthProblem, AuthState, RepositoryList, RepositoryListState,
};
use crate::app_info::AppInfo;
use crate::github::octocrab_client::OctocrabGateway;
use crate::github::{Repository, RepositorySort, filter_repositories};
use crate::runtime::TokioRuntime;
use crate::store::Store;
use crate::strings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkspaceTab {
    Workflows,
    Runs,
}

struct AppView {
    info: AppInfo,
    manager: Arc<Mutex<AuthManager>>,
    picker: Arc<Mutex<RepositoryList>>,
    runtime: TokioRuntime,
    auth: AuthState,
    copied: bool,
    pat_input: Entity<InputState>,
    search_input: Entity<InputState>,
    pat_subscription: Option<Subscription>,
    repos: Vec<Repository>,
    repo_state: RepositoryListState,
    repo_has_more: bool,
    repo_sort: RepositorySort,
    selected: Option<String>,
    workspace_tab: WorkspaceTab,
}

impl AppView {
    fn new(
        info: AppInfo,
        manager: Arc<Mutex<AuthManager>>,
        picker: Arc<Mutex<RepositoryList>>,
        runtime: TokioRuntime,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let pat_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(strings::LOGIN_PAT_PLACEHOLDER, window, cx);
            state
        });
        let search_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(strings::REPOSITORIES_SEARCH_PLACEHOLDER, window, cx);
            state
        });

        Self {
            info,
            manager,
            picker,
            runtime,
            auth: AuthState::LoggedOut { notice: None },
            copied: false,
            pat_input,
            search_input,
            pat_subscription: None,
            repos: Vec::new(),
            repo_state: RepositoryListState::Idle,
            repo_has_more: false,
            repo_sort: RepositorySort::Updated,
            selected: None,
            workspace_tab: WorkspaceTab::Workflows,
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
        manager: &Arc<Mutex<AuthManager>>,
        picker: &Arc<Mutex<RepositoryList>>,
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
    }

    fn restore(&mut self, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
        let picker = self.picker.clone();
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
                Self::load_repositories(&manager, &picker, &runtime, &this, cx).await;
            }
        })
        .detach();
    }

    fn start_login(&mut self, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
        let picker = self.picker.clone();
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
                        Self::load_repositories(&manager, &picker, &runtime, &this, cx).await;
                    }
                    break;
                }
            }
        })
        .detach();
    }

    fn submit_pat(&mut self, cx: &mut Context<Self>) {
        let raw = self.pat_input.read(cx).value().to_string();
        let manager = self.manager.clone();
        let picker = self.picker.clone();
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
                Self::load_repositories(&manager, &picker, &runtime, &this, cx).await;
            }
        })
        .detach();
    }

    fn sign_out(&mut self, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
        let picker = self.picker.clone();
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
        let picker = self.picker.clone();
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

            let selected = { picker.lock().await.selected().map(str::to_owned) };
            let _ = this.update(cx, |this, cx| {
                this.selected = selected;
                this.workspace_tab = WorkspaceTab::Workflows;
                cx.notify();
            });
        })
        .detach();
    }

    fn leave_workspace(&mut self, cx: &mut Context<Self>) {
        let picker = self.picker.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let picker = picker.clone();
                async move {
                    picker.lock().await.leave_workspace().await;
                }
            });
            let _ = task.await;

            let _ = this.update(cx, |this, cx| {
                this.selected = None;
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
        AuthProblem::DeviceFlowUnavailable => strings::NOTICE_DEVICE_FLOW_UNAVAILABLE,
        AuthProblem::Expired => strings::NOTICE_EXPIRED,
        AuthProblem::Denied => strings::NOTICE_DENIED,
        AuthProblem::InvalidCredentials => strings::NOTICE_INVALID_CREDENTIALS,
        AuthProblem::MissingScopes => strings::NOTICE_MISSING_SCOPES,
        AuthProblem::Network => strings::NOTICE_NETWORK,
        AuthProblem::Unexpected => strings::NOTICE_UNEXPECTED,
    }
}

fn problem_text(problem: AppProblem) -> &'static str {
    match problem {
        AppProblem::Forbidden => strings::NOTICE_MISSING_SCOPES,
        AppProblem::RateLimited => strings::PROBLEM_RATE_LIMITED,
        AppProblem::NotFound => strings::PROBLEM_NOT_FOUND,
        AppProblem::Network => strings::NOTICE_NETWORK,
        AppProblem::Unexpected => strings::NOTICE_UNEXPECTED,
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
        let packaging_config = self.info.packaging_config().unwrap_or(strings::UNSPECIFIED);

        div()
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .child(Label::new(strings::APP_TITLE))
            .child(info_row(strings::LABEL_VERSION, self.info.version()))
            .child(info_row(
                strings::LABEL_BUILD_TARGET,
                self.info.build_target(),
            ))
            .child(info_row(strings::LABEL_PACKAGING_CONFIG, packaging_config))
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
                        strings::LOGIN_LOGGED_IN_AS,
                        account.login
                    )))
                    .child(
                        Button::new("sign-out")
                            .label(strings::LOGIN_SIGN_OUT)
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
                            .label(strings::LOGIN_START)
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| this.start_login(cx))),
                    );
            }
            AuthState::StartingDeviceFlow => {
                panel = panel.child(Label::new(strings::LOGIN_STARTING));
            }
            AuthState::AwaitingAuthorization { start } => {
                let code = start.user_code.clone();
                let uri = start.verification_uri.clone();
                panel = panel
                    .child(Label::new(strings::LOGIN_INSTRUCTION))
                    .child(Label::new(start.verification_uri.clone()))
                    .child(Label::new(format!(
                        "{}：{}",
                        strings::LOGIN_USER_CODE,
                        start.user_code
                    )))
                    .child(Label::new(format!(
                        "{}：{}",
                        strings::LOGIN_EXPIRES_IN,
                        start.expires_in_secs
                    )))
                    .child(Label::new(strings::LOGIN_WAITING))
                    .child(Label::new(if self.copied {
                        strings::LOGIN_COPIED
                    } else {
                        ""
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_3()
                            .child(
                                Button::new("copy-code")
                                    .label(strings::LOGIN_COPY)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            code.clone(),
                                        ));
                                        this.copied = true;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("open-verification")
                                    .label(strings::LOGIN_OPEN_BROWSER)
                                    .on_click(move |_, _, _| {
                                        let _ = open::that(uri.clone());
                                    }),
                            ),
                    );
            }
            AuthState::ValidatingCredentials => {
                panel = panel.child(Label::new(strings::LOGIN_VALIDATING));
            }
            AuthState::Authenticated { .. } => {}
        }

        panel
            .child(Label::new(strings::LOGIN_PAT_TITLE))
            .child(Input::new(&self.pat_input))
            .child(
                Button::new("submit-pat")
                    .label(strings::LOGIN_PAT_SUBMIT)
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
                    strings::REPOSITORIES_PRIVATE
                } else {
                    strings::REPOSITORIES_PUBLIC
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
                list = list.child(Label::new(strings::REPOSITORIES_LOADING));
            }
            RepositoryListState::Failed(problem) => {
                list = list.child(Label::new(problem_text(problem)).text_sm());
            }
            RepositoryListState::Loaded if visible.is_empty() => {
                list = list.child(Label::new(strings::REPOSITORIES_EMPTY));
            }
            _ => {}
        }

        if self.repo_has_more {
            list = list.child(
                Button::new("load-more")
                    .label(strings::REPOSITORIES_LOAD_MORE)
                    .on_click(cx.listener(|this, _, _, cx| this.load_more(cx))),
            );
        }

        let updated_selected = self.repo_sort == RepositorySort::Updated;
        let mut sort_updated =
            Button::new("sort-updated").label(strings::REPOSITORIES_SORT_UPDATED);
        if updated_selected {
            sort_updated = sort_updated.primary();
        }
        let mut sort_pushed = Button::new("sort-pushed").label(strings::REPOSITORIES_SORT_PUSHED);
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
            .child(Label::new(strings::REPOSITORIES_TITLE))
            .child(Input::new(&self.search_input))
            .child(sort_row)
            .child(list)
            .into_any_element()
    }

    fn workspace_shell(&self, full_name: &str, cx: &mut Context<Self>) -> AnyElement {
        let mut workflows_tab = Button::new("tab-workflows").label(strings::WORKSPACE_WORKFLOWS);
        if self.workspace_tab == WorkspaceTab::Workflows {
            workflows_tab = workflows_tab.primary();
        }
        let mut runs_tab = Button::new("tab-runs").label(strings::WORKSPACE_RUNS);
        if self.workspace_tab == WorkspaceTab::Runs {
            runs_tab = runs_tab.primary();
        }

        let tabs = div()
            .flex()
            .flex_row()
            .gap_2()
            .child(workflows_tab.on_click(cx.listener(|this, _, _, cx| {
                this.workspace_tab = WorkspaceTab::Workflows;
                cx.notify();
            })))
            .child(runs_tab.on_click(cx.listener(|this, _, _, cx| {
                this.workspace_tab = WorkspaceTab::Runs;
                cx.notify();
            })));

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .child(
                Button::new("back-to-repos")
                    .label(strings::WORKSPACE_BACK)
                    .on_click(cx.listener(|this, _, _, cx| this.leave_workspace(cx))),
            )
            .child(Label::new(full_name.to_owned()))
            .child(tabs)
            .child(Label::new(strings::WORKSPACE_COMING_SOON))
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
    let gateway = Arc::new(OctocrabGateway::new(
        option_env!("GITHUB_CLIENT_ID").map(str::to_owned),
    ));
    let manager = Arc::new(Mutex::new(AuthManager::new(gateway.clone(), store.clone())));
    let picker = Arc::new(Mutex::new(RepositoryList::new(gateway, store)));

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        let info = info.clone();
        let manager = manager.clone();
        let picker = picker.clone();
        let runtime = runtime.clone();

        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some(strings::APP_TITLE.into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    Theme::sync_system_appearance(Some(window), cx);
                    let view =
                        cx.new(|cx| AppView::new(info, manager, picker, runtime, window, cx));
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
