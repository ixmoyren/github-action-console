use super::AppView;

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::dialog::DialogButtonProps;

use super::settings::SettingsWindow;

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
        self.search_subscription = Some(cx.subscribe(
            &self.search_input,
            |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.refresh_repo_table(cx);
                    cx.notify();
                }
            },
        ));
        self.branch_subscription = Some(cx.subscribe(
            &self.branch_input,
            |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.refresh_run_table(cx);
                    cx.notify();
                }
            },
        ));
    }
    pub(super) async fn refresh_picker(
        picker: &Arc<Mutex<RepositoryList>>,
        this: &WeakEntity<AppView>,
        cx: &mut AsyncApp,
    ) {
        let (state, repos, has_more, selected) = {
            let guard = picker.lock().await;
            (
                guard.state(),
                guard.visible(),
                guard.has_more(),
                guard.selected().map(str::to_owned),
            )
        };

        if let Err(error) = this.update(cx, |this, cx| {
            this.repo_state = state;
            this.repos = repos;
            this.repo_has_more = has_more;
            this.selected = selected;
            this.refresh_repo_table(cx);
            cx.notify();
        }) {
            warn!(?error, "the view was gone before the update landed");
        };
    }
    pub(super) async fn load_repositories(
        gateway: &Arc<dyn GitHubGateway>,
        token: &SecretToken,
        handles: &ScopedHandles,
        runtime: &TokioRuntime,
        this: &WeakEntity<AppView>,
        cx: &mut AsyncApp,
    ) {
        let ScopedHandles {
            picker,
            workspace,
            board: _,
        } = handles;
        let token = token.clone();

        let task = runtime.spawn({
            let picker = picker.clone();
            async move {
                picker.lock().await.restore_selection().await;
            }
        });
        if let Err(error) = task.await {
            warn!(%error, "a background task did not finish");
        }

        let task = runtime.spawn({
            let picker = picker.clone();
            let token = token.clone();
            async move {
                picker.lock().await.reload(&token).await;
            }
        });
        if let Err(error) = task.await {
            warn!(%error, "a background task did not finish");
        }

        Self::refresh_picker(picker, this, cx).await;

        // Reopen the remembered repository, if there is one.
        let remembered = { picker.lock().await.selected().map(str::to_owned) };
        if let Some(full_name) = remembered {
            let task = runtime.spawn({
                let picker = picker.clone();
                let workspace = workspace.clone();
                let full_name = full_name.clone();
                async move {
                    let default_branch = picker.lock().await.default_branch_of(&full_name);
                    workspace.lock().await.enter(&full_name, default_branch);
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            Self::load_workspace(gateway, &token.clone(), handles, runtime, this, cx).await;
        }
    }
    pub(super) fn restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
        let runtime = self.runtime.clone();
        let view = cx.entity();

        // Window-scoped task: it runs on the GPUI thread with the window in
        // hand, which is the only context where gpui-kit dialogs can open.
        cx.spawn_in(window, async move |this, cx| {
            let task = runtime.spawn({
                let manager = manager.clone();
                async move {
                    manager.lock().await.restore_session().await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "the session restore task did not finish");
            }

            let state = manager.lock().await.state().clone();
            if !matches!(state, AuthState::Authenticated { .. }) {
                if let Err(error) = this.update(cx, |this, cx| {
                    this.auth = state;
                    cx.notify();
                }) {
                    warn!(?error, "the view was gone before the update landed");
                }
                return;
            }

            info!("a stored session is still valid; asking the user");
            if let Err(error) = this.update(cx, |this, cx| {
                this.auth = state;
                this.resume_prompt = true;
                cx.notify();
            }) {
                warn!(?error, "the view was gone before the update landed");
            }

            let accept = view.clone();
            let decline = view.clone();
            let opened = cx.update(|window, cx| {
                window.open_alert_dialog(cx, move |alert, _window, _cx| {
                    info!("resume dialog builder ran");
                    let accept = accept.clone();
                    let decline = decline.clone();
                    alert
                        .button_props(
                            DialogButtonProps::default()
                                .ok_text(labels::RESUME_YES)
                                .cancel_text(labels::RESUME_NO),
                        )
                        .show_cancel(true)
                        .title(labels::RESUME_TITLE)
                        .description(pickable(labels::RESUME_BODY))
                        .on_ok(move |_, _, cx| {
                            accept.update(cx, |this, cx| this.enter_after_resume(cx));
                            true
                        })
                        .on_cancel(move |_, _, cx| {
                            decline.update(cx, |this, cx| this.decline_resume(cx));
                            true
                        })
                });
            });
            match opened {
                Ok(()) => info!("resume dialog handed to Root"),
                Err(error) => warn!(%error, "could not open the resume dialog"),
            }
        })
        .detach();
    }
    pub(super) fn start_login(&mut self, cx: &mut Context<Self>) {
        let client_id = self.client_id_input.read(cx).value().to_string();
        info!("device-flow sign in requested");
        self.login_step = LoginStep::ClientIdEntry;
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let picker = self.picker.clone();
        let workspace = self.workspace.clone();
        let board = self.board.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let manager = manager.clone();
                async move {
                    manager.lock().await.start_device_flow(&client_id).await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            let state = manager.lock().await.state().clone();
            let awaiting = matches!(state, AuthState::AwaitingAuthorization { .. });
            let remaining = manager.lock().await.remaining_secs();
            if let Err(error) = this.update(cx, |this, cx| {
                this.auth = state;
                this.copied = false;
                this.flow_remaining_secs = remaining;
                // Only move off the client-id page once GitHub accepted it.
                if awaiting {
                    this.login_step = LoginStep::DeviceFlow;
                }
                cx.notify();
            }) {
                warn!(?error, "the view was gone before the update landed");
            };

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
                    if let Err(error) = this.update(cx, |this, cx| {
                        this.flow_remaining_secs = remaining;
                        cx.notify();
                    }) {
                        warn!(?error, "the view was gone before the update landed");
                    };
                }

                let task = runtime.spawn({
                    let manager = manager.clone();
                    async move {
                        manager.lock().await.poll_device_flow().await;
                    }
                });
                if let Err(error) = task.await {
                    warn!(%error, "a background task did not finish");
                }

                let state = manager.lock().await.state().clone();
                let remaining = manager.lock().await.remaining_secs();
                let finished = !matches!(state, AuthState::AwaitingAuthorization { .. });
                let authenticated = matches!(state, AuthState::Authenticated { .. });
                if let Err(error) = this.update(cx, |this, cx| {
                    this.auth = state;
                    this.flow_remaining_secs = remaining;
                    cx.notify();
                }) {
                    warn!(?error, "the view was gone before the update landed");
                };
                if finished {
                    if authenticated {
                        let token = { manager.lock().await.token() };
                        let Some(token) = token else {
                            return;
                        };
                        let handles = ScopedHandles {
                            picker: picker.clone(),
                            workspace: workspace.clone(),
                            board: board.clone(),
                        };
                        Self::load_repositories(&gateway, &token, &handles, &runtime, &this, cx)
                            .await;
                        let login = match manager.lock().await.state() {
                            AuthState::Authenticated { account } => Some(account.login.clone()),
                            _ => None,
                        };
                        if let Err(error) =
                            this.update(cx, |this, cx| this.refresh_status_bar(login, cx))
                        {
                            warn!(?error, "the view was gone before the update landed");
                        };
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
        let board = self.board.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let manager = manager.clone();
                async move {
                    manager.lock().await.sign_in_with_token(&raw).await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            let state = manager.lock().await.state().clone();
            let authenticated = matches!(state, AuthState::Authenticated { .. });
            if let Err(error) = this.update(cx, |this, cx| {
                this.auth = state;
                cx.notify();
            }) {
                warn!(?error, "the view was gone before the update landed");
            };

            if authenticated {
                let token = { manager.lock().await.token() };
                let Some(token) = token else {
                    return;
                };
                let handles = ScopedHandles {
                    picker: picker.clone(),
                    workspace: workspace.clone(),
                    board: board.clone(),
                };
                Self::load_repositories(&gateway, &token, &handles, &runtime, &this, cx).await;
                let login = match manager.lock().await.state() {
                    AuthState::Authenticated { account } => Some(account.login.clone()),
                    _ => None,
                };
                if let Err(error) = this.update(cx, |this, cx| this.refresh_status_bar(login, cx)) {
                    warn!(?error, "the view was gone before the update landed");
                };
            }
        })
        .detach();
    }
    /// The user accepted the stored session: open the repository list.
    pub(super) fn enter_after_resume(&mut self, cx: &mut Context<Self>) {
        info!("resuming the stored session");
        self.resume_prompt = false;

        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let picker = self.picker.clone();
        let workspace = self.workspace.clone();
        let board = self.board.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };
            let handles = ScopedHandles {
                picker: picker.clone(),
                workspace: workspace.clone(),
                board: board.clone(),
            };
            Self::load_repositories(&gateway, &token, &handles, &runtime, &this, cx).await;
            let login = match manager.lock().await.state() {
                AuthState::Authenticated { account } => Some(account.login.clone()),
                _ => None,
            };
            let _ = this.update(cx, |this, cx| this.refresh_status_bar(login, cx));
        })
        .detach();
    }

    /// The user declined the stored session: sign out and stay on the home page.
    pub(super) fn decline_resume(&mut self, cx: &mut Context<Self>) {
        info!("declining the stored session; signing out");
        self.resume_prompt = false;
        self.sign_out(cx);
    }

    pub(super) fn sign_out(&mut self, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
        let picker = self.picker.clone();
        let workspace = self.workspace.clone();
        let board = self.board.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let manager = manager.clone();
                async move {
                    manager.lock().await.sign_out().await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            let task = runtime.spawn({
                let picker = picker.clone();
                async move {
                    picker.lock().await.leave_workspace().await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            let task = runtime.spawn({
                let workspace = workspace.clone();
                let board = board.clone();
                async move {
                    workspace.lock().await.leave();
                    board.lock().await.leave();
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            if let Err(error) = this.update(cx, |this, cx| {
                this.auth = AuthState::LoggedOut { notice: None };
                this.login_step = LoginStep::Home;
                this.resume_prompt = false;
                this.selected = None;
                this.repos.clear();
                this.repo_state = RepositoryListState::Idle;
                this.workspace_tab = WorkspaceTab::Workflows;
                this.refresh_repo_table(cx);
                cx.notify();
            }) {
                warn!(?error, "the view was gone before the update landed");
            };
        })
        .detach();
    }
    pub(super) fn login_page(&self, state: AuthState, cx: &mut Context<Self>) -> AnyElement {
        let (show_back, content) = match self.login_step {
            LoginStep::Home => (false, self.login_home(cx)),
            LoginStep::PatEntry => (true, self.pat_entry(cx)),
            LoginStep::ClientIdEntry => (true, self.client_id_entry(cx)),
            LoginStep::DeviceFlow => (true, self.device_flow_page(&state, cx)),
        };

        let back = if show_back {
            Button::new("back-to-login-home")
                .label(labels::LOGIN_PAT_BACK)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.login_step = LoginStep::Home;
                    this.flow_remaining_secs = None;
                    if let AuthState::LoggedOut { notice } = &mut this.auth {
                        *notice = None;
                    }
                    cx.notify();
                }))
                .into_any_element()
        } else {
            div().into_any_element()
        };

        let settings = if matches!(self.login_step, LoginStep::Home) {
            Button::new("open-settings")
                .icon(IconName::Settings)
                .on_click(cx.listener(|this, _, window, cx| this.open_settings_window(window, cx)))
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

    /// Persist the proxy and point the gateway at it.
    /// Open the settings child window: borderless, centred on this window,
    /// one third as wide and two fifths as tall.
    pub(super) fn open_settings_window(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // Zed's settings window uses gpui's standard additional-window size and
        // lets the platform centre it; ours is half of that (900x750 -> 450x375).
        let base = DEFAULT_ADDITIONAL_WINDOW_SIZE;
        let size = Size {
            width: base.width / 2.0,
            height: base.height / 2.0,
        };

        let current = self.proxy_input.read(cx).value().to_string();
        let initial_proxy = {
            let trimmed = current.trim().to_owned();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        };

        let store = self.store.clone();
        let gateway = self.gateway.clone();
        let runtime = self.runtime.clone();

        let window_bounds = WindowBounds::centered(size, cx);

        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(window_bounds),
                window_min_size: Some(Size {
                    width: px(360.0),
                    height: px(240.0),
                }),
                // Borderless: the window draws its own title bar.
                titlebar: None,
                is_resizable: false,
                is_movable: true,
                focus: true,
                show: true,
                app_id: Some("github-action-console".to_owned()),
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            },
            move |window, cx| {
                Theme::sync_system_appearance(Some(window), cx);
                let view = cx.new(|cx| {
                    SettingsWindow::new(store, gateway, runtime, initial_proxy, window, cx)
                });
                cx.new(|cx| Root::new(view, window, cx))
            },
        );

        match opened {
            Ok(_) => info!("settings window opened"),
            Err(error) => warn!(%error, "could not open the settings window"),
        }
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
