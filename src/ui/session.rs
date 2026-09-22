use super::AppView;
use super::*;

impl AppView {
    pub(super) fn wire(&mut self, cx: &mut Context<Self>) {
        self.pat_subscription = Some(cx.subscribe(
            &self.pat_input,
            |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.submit_pat(cx);
                }
            },
        ));
    }
    pub(super) async fn refresh_picker(
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
    pub(super) async fn load_repositories(
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
    pub(super) fn restore(&mut self, cx: &mut Context<Self>) {
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
                let login = match manager.lock().await.state() {
                    AuthState::Authenticated { account } => Some(account.login.clone()),
                    _ => None,
                };
                let _ = this.update(cx, |this, cx| this.refresh_status_bar(login, cx));
            }
        })
        .detach();
    }
    pub(super) fn start_login(&mut self, cx: &mut Context<Self>) {
        let client_id = self.client_id_input.read(cx).value().to_string();
        self.login_step = LoginStep::ClientIdEntry;
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let picker = self.picker.clone();
        let workspace = self.workspace.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let manager = manager.clone();
                async move {
                    manager.lock().await.start_device_flow(&client_id).await;
                }
            });
            let _ = task.await;

            let state = manager.lock().await.state().clone();
            let awaiting = matches!(state, AuthState::AwaitingAuthorization { .. });
            let remaining = manager.lock().await.remaining_secs();
            let _ = this.update(cx, |this, cx| {
                this.auth = state;
                this.copied = false;
                this.flow_remaining_secs = remaining;
                // Only move off the client-id page once GitHub accepted it.
                if awaiting {
                    this.login_step = LoginStep::DeviceFlow;
                }
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

                // Wait out the polling interval a second at a time so the
                // validity countdown on screen actually moves.
                let mut waited = Duration::ZERO;
                while waited < interval {
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                    waited += Duration::from_secs(1);

                    let remaining = manager.lock().await.remaining_secs();
                    let _ = this.update(cx, |this, cx| {
                        this.flow_remaining_secs = remaining;
                        cx.notify();
                    });
                }

                let task = runtime.spawn({
                    let manager = manager.clone();
                    async move {
                        manager.lock().await.poll_device_flow().await;
                    }
                });
                let _ = task.await;

                let state = manager.lock().await.state().clone();
                let remaining = manager.lock().await.remaining_secs();
                let finished = !matches!(state, AuthState::AwaitingAuthorization { .. });
                let authenticated = matches!(state, AuthState::Authenticated { .. });
                let _ = this.update(cx, |this, cx| {
                    this.auth = state;
                    this.flow_remaining_secs = remaining;
                    cx.notify();
                });
                if finished {
                    if authenticated {
                        Self::load_repositories(
                            &gateway, &manager, &picker, &workspace, &runtime, &this, cx,
                        )
                        .await;
                        let login = match manager.lock().await.state() {
                            AuthState::Authenticated { account } => Some(account.login.clone()),
                            _ => None,
                        };
                        let _ = this.update(cx, |this, cx| this.refresh_status_bar(login, cx));
                    }
                    break;
                }
            }
        })
        .detach();
    }
    pub(super) fn submit_pat(&mut self, cx: &mut Context<Self>) {
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
                let login = match manager.lock().await.state() {
                    AuthState::Authenticated { account } => Some(account.login.clone()),
                    _ => None,
                };
                let _ = this.update(cx, |this, cx| this.refresh_status_bar(login, cx));
            }
        })
        .detach();
    }
    pub(super) fn sign_out(&mut self, cx: &mut Context<Self>) {
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
                this.login_step = LoginStep::Home;
                this.selected = None;
                this.repos.clear();
                this.repo_state = RepositoryListState::Idle;
                this.workspace_tab = WorkspaceTab::Workflows;
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn account_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        match &self.auth {
            AuthState::Authenticated { account } => Some(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .p_3()
                    .child(pickable(format!(
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
    pub(super) fn login_page(&self, state: AuthState, cx: &mut Context<Self>) -> AnyElement {
        let settings_open = self.settings_open;
        let (show_back, content) = if settings_open {
            (false, self.settings_panel(cx))
        } else {
            match self.login_step {
                LoginStep::Home => (false, self.login_home(cx)),
                LoginStep::PatEntry => (true, self.pat_entry(cx)),
                LoginStep::ClientIdEntry => (true, self.client_id_entry(cx)),
                LoginStep::DeviceFlow => (true, self.device_flow_page(&state, cx)),
            }
        };

        let back = if show_back {
            Button::new("back-to-login-home")
                .label(labels::LOGIN_PAT_BACK)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.login_step = LoginStep::Home;
                    this.flow_remaining_secs = None;
                    this.settings_open = false;
                    if let AuthState::LoggedOut { notice } = &mut this.auth {
                        *notice = None;
                    }
                    cx.notify();
                }))
                .into_any_element()
        } else {
            div().into_any_element()
        };

        let settings = if matches!(self.login_step, LoginStep::Home) && !settings_open {
            Button::new("open-settings")
                .icon(IconName::Settings)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.settings_open = true;
                    cx.notify();
                }))
                .into_any_element()
        } else {
            div().into_any_element()
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .p_3()
                    .child(back)
                    .child(settings),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(content),
            )
            .into_any_element()
    }

    /// The settings modal content: proxy address, save, close.
    fn settings_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(pickable(labels::SETTINGS_PROXY_LABEL))
                    .child(Input::new(&self.proxy_input).w(px(360.0))),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_3()
                    .child(
                        Button::new("save-settings")
                            .label(labels::SETTINGS_SAVE)
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| this.save_proxy(cx))),
                    )
                    .child(
                        Button::new("close-settings")
                            .label(labels::SETTINGS_CLOSE)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings_open = false;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    /// Persist the proxy and point the gateway at it.
    pub(super) fn save_proxy(&mut self, cx: &mut Context<Self>) {
        let proxy = self.proxy_input.read(cx).value().to_string();
        let store = self.store.clone();
        let gateway = self.gateway.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let store = store.clone();
                let gateway = gateway.clone();
                let proxy = proxy.clone();
                async move {
                    let cleaned = proxy.trim().to_owned();
                    let value = if cleaned.is_empty() {
                        None
                    } else {
                        Some(cleaned)
                    };
                    store.save_proxy(value.as_deref()).await.ok();
                    gateway.set_proxy(value);
                }
            });
            let _ = task.await;

            let _ = this.update(cx, |this, cx| {
                this.settings_open = false;
                cx.notify();
            });
        })
        .detach();
    }

    /// The signed-out landing page: exactly two entry buttons, side by side.
    fn login_home(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .child(
                Button::new("open-client-id")
                    .label(labels::LOGIN_START)
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.login_step = LoginStep::ClientIdEntry;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("toggle-pat")
                    .label(labels::LOGIN_PAT_BUTTON)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.login_step = LoginStep::PatEntry;
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    /// The Personal Access Token page: one field, one confirm button.
    fn pat_entry(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = div().flex().flex_col().items_center().gap_3();
        if let AuthState::LoggedOut {
            notice: Some(problem),
        } = self.auth
        {
            panel = panel.child(Label::new(notice_text(problem)).text_sm());
        }

        panel
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .w(px(360.0))
                    .child(Input::new(&self.pat_input).flex_1())
                    .child(
                        Button::new("submit-pat")
                            .label(labels::LOGIN_PAT_SUBMIT_ARROW)
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| this.submit_pat(cx))),
                    ),
            )
            .into_any_element()
    }

    /// The OAuth App client-id page: one field, one confirm button.
    fn client_id_entry(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = div().flex().flex_col().items_center().gap_3();
        if let AuthState::LoggedOut {
            notice: Some(problem),
        } = self.auth
        {
            panel = panel.child(Label::new(notice_text(problem)).text_sm());
        }

        panel
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .w(px(360.0))
                    .child(Input::new(&self.client_id_input).flex_1())
                    .child(
                        Button::new("submit-client-id")
                            .label(labels::LOGIN_PAT_SUBMIT_ARROW)
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| this.start_login(cx))),
                    ),
            )
            .into_any_element()
    }

    /// The device-flow page: waiting for GitHub, or the failure that ended it.
    fn device_flow_page(&self, state: &AuthState, cx: &mut Context<Self>) -> AnyElement {
        match state {
            AuthState::StartingDeviceFlow => div()
                .child(pickable(labels::LOGIN_STARTING))
                .into_any_element(),
            AuthState::AwaitingAuthorization { start } => {
                let code = start.user_code.clone();
                let uri = start.verification_uri.clone();

                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .child(pickable(labels::LOGIN_INSTRUCTION))
                    .child(pickable(start.verification_uri.clone()))
                    .child(pickable(format!(
                        "{}：{}",
                        labels::LOGIN_USER_CODE,
                        start.user_code
                    )))
                    .child(pickable(format!(
                        "{}：{}",
                        labels::LOGIN_EXPIRES_IN,
                        self.flow_remaining_secs.unwrap_or(start.expires_in_secs)
                    )))
                    .child(pickable(labels::LOGIN_WAITING))
                    .child(pickable(if self.copied {
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
                    )
                    .into_any_element()
            }
            AuthState::LoggedOut {
                notice: Some(problem),
            } => div()
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .child(Label::new(notice_text(*problem)).text_sm())
                .into_any_element(),
            _ => div().into_any_element(),
        }
    }
}
