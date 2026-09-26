use super::AppView;

use super::*;

pub(crate) fn notice_text(problem: AuthProblem) -> &'static str {
    match problem {
        AuthProblem::DeviceFlowUnavailable => labels::NOTICE_DEVICE_FLOW_UNAVAILABLE,
        AuthProblem::MissingClientId => labels::NOTICE_MISSING_CLIENT_ID,
        AuthProblem::Expired => labels::NOTICE_EXPIRED,
        AuthProblem::Denied => labels::NOTICE_DENIED,
        AuthProblem::InvalidCredentials => labels::NOTICE_INVALID_CREDENTIALS,
        AuthProblem::MissingScopes => labels::NOTICE_MISSING_SCOPES,
        AuthProblem::Network => labels::NOTICE_NETWORK,
        AuthProblem::Unexpected => labels::NOTICE_UNEXPECTED,
    }
}

pub(crate) fn problem_text(problem: AppProblem) -> &'static str {
    match problem {
        AppProblem::Forbidden => labels::NOTICE_MISSING_SCOPES,
        AppProblem::RateLimited => labels::PROBLEM_RATE_LIMITED,
        AppProblem::NotFound => labels::PROBLEM_NOT_FOUND,
        AppProblem::Network => labels::NOTICE_NETWORK,
        AppProblem::Unexpected => labels::NOTICE_UNEXPECTED,
    }
}

impl AppView {}

/// Text the user can select and copy. IDs are handed out in render order, so
/// they stay stable while a screen's element set is stable.
pub(crate) fn pickable(text: impl Into<gpui_kit::SharedString>) -> gpui_kit::base::SelectableText {
    gpui_kit::base::SelectableText::new(
        PICKABLE_NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        text,
    )
}

/// Restart the id sequence. Called once at the top of `render`.
pub(crate) fn reset_pickable_ids() {
    PICKABLE_NEXT.store(0, std::sync::atomic::Ordering::Relaxed);
}

static PICKABLE_NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        reset_pickable_ids();
        // The workflow file is read in the background, but the editor can only
        // take text with a window in hand. This is where the two meet.
        self.sync_yaml_editor(window, cx);

        // The window title carries the signed-in user; the app chrome no longer
        // repeats it anywhere on screen.
        let title = match &self.auth {
            AuthState::Authenticated { account } => {
                format!("{} ~ {}", labels::APP_TITLE, account.login)
            }
            _ => labels::APP_TITLE.to_owned(),
        };
        if self.window_title.as_deref() != Some(title.as_str()) {
            window.set_window_title(&title);
            self.window_title = Some(title);
        }

        let signed_in = matches!(self.auth, AuthState::Authenticated { .. }) && !self.resume_prompt;
        if !signed_in {
            let login = self.login_page(self.auth.clone(), cx);
            return div()
                .relative()
                .size_full()
                .child(login)
                // gpui-component's Root does not draw the dialog layer; the
                // app view has to render it.
                .children(Root::render_dialog_layer(window, cx))
                .into_any_element();
        }

        let leading = match self.selected.clone() {
            Some(full_name) => div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    Button::new("back-to-repos")
                        .label(labels::WORKSPACE_BACK_ARROW)
                        .tooltip(labels::WORKSPACE_BACK)
                        .on_click(cx.listener(|this, _, _, cx| this.leave_workspace(cx))),
                )
                .child(pickable(full_name))
                .into_any_element(),
            None => div().into_any_element(),
        };

        let top_bar = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .p_3()
            .child(leading)
            .child(
                Button::new("sign-out")
                    .label(labels::LOGIN_SIGN_OUT)
                    .on_click(cx.listener(|this, _, _, cx| this.sign_out(cx))),
            );

        let content = match self.selected.clone() {
            Some(_) => self.workspace_shell(cx),
            None => self.repository_picker(cx),
        };

        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .child(top_bar)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(content),
            )
            .child(self.status_bar(cx))
            .children(Root::render_dialog_layer(window, cx))
            .into_any_element()
    }
}
