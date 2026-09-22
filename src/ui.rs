use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Root, Theme, label::Label};
use gpui_kit::*;
use tokio::sync::Mutex;

use crate::app::{AuthManager, AuthProblem, AuthState};
use crate::app_info::AppInfo;
use crate::github::octocrab_client::OctocrabGateway;
use crate::runtime::TokioRuntime;
use crate::store::Store;
use crate::strings;

struct AppView {
    info: AppInfo,
    manager: Arc<Mutex<AuthManager>>,
    runtime: TokioRuntime,
    auth: AuthState,
    copied: bool,
}

impl AppView {
    fn new(info: AppInfo, manager: Arc<Mutex<AuthManager>>, runtime: TokioRuntime) -> Self {
        Self {
            info,
            manager,
            runtime,
            auth: AuthState::LoggedOut { notice: None },
            copied: false,
        }
    }

    /// Resume a previous session from the keyring, then let the caller decide
    /// what to show.
    fn restore(&mut self, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
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
            let _ = this.update(cx, |this, cx| {
                this.auth = state;
                cx.notify();
            });
        })
        .detach();
    }

    fn start_login(&mut self, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
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
                let _ = this.update(cx, |this, cx| {
                    this.auth = state;
                    cx.notify();
                });
                if finished {
                    break;
                }
            }
        })
        .detach();
    }

    fn sign_out(&mut self, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let manager = manager.clone();
                async move {
                    manager.lock().await.sign_out().await;
                }
            });
            let _ = task.await;

            let _ = this.update(cx, |this, cx| {
                this.auth = AuthState::LoggedOut { notice: None };
                cx.notify();
            });
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
        AuthProblem::Network => strings::NOTICE_NETWORK,
        AuthProblem::Unexpected => strings::NOTICE_UNEXPECTED,
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

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let packaging_config = self.info.packaging_config().unwrap_or(strings::UNSPECIFIED);

        let header = div()
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
            .child(info_row(strings::LABEL_PACKAGING_CONFIG, packaging_config));

        let body = match self.auth.clone() {
            AuthState::Authenticated { account } => div()
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
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

            AuthState::StartingDeviceFlow => div()
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .child(Label::new(strings::LOGIN_STARTING))
                .into_any_element(),

            AuthState::AwaitingAuthorization { start } => {
                let code = start.user_code.clone();
                let uri = start.verification_uri.clone();
                let copied = self.copied;

                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
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
                    .child(Label::new(if copied { strings::LOGIN_COPIED } else { "" }))
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
                    )
                    .into_any_element()
            }

            AuthState::LoggedOut { notice } => div()
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .children(notice.map(|problem| Label::new(notice_text(problem)).text_sm()))
                .child(
                    Button::new("start-login")
                        .label(strings::LOGIN_START)
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| this.start_login(cx))),
                )
                .into_any_element(),
        };

        div().size_full().flex().flex_col().child(header).child(
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
    let manager = Arc::new(Mutex::new(AuthManager::new(gateway, store)));

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        let info = info.clone();
        let manager = manager.clone();
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
                    let view = cx.new(|_| AppView::new(info, manager, runtime));
                    view.update(cx, |this, cx| this.restore(cx));
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("failed to open window");
        })
        .detach();
    });

    Ok(())
}
