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

pub(super) fn info_row(label: &'static str, value: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(Label::new(label).text_sm())
        .child(pickable(value.to_owned()))
}

impl AppView {
    pub(super) fn header(&self) -> impl IntoElement {
        let packaging_config = self.info.packaging_config().unwrap_or(labels::UNSPECIFIED);

        div()
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .child(pickable(labels::APP_TITLE))
            .child(info_row(labels::LABEL_VERSION, self.info.version()))
            .child(info_row(
                labels::LABEL_BUILD_TARGET,
                self.info.build_target(),
            ))
            .child(info_row(labels::LABEL_PACKAGING_CONFIG, packaging_config))
    }
}

/// Text the user can select and copy. IDs are handed out in render order, so
/// they stay stable while a screen's element set is stable.
pub(crate) fn pickable(text: impl Into<gpui_kit::SharedString>) -> gpui_kit::base::SelectableText {
    gpui_kit::base::SelectableText::new(
        PICKABLE_NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        text,
    )
}

/// Restart the id sequence. Called once at the top of `render`.
pub(super) fn reset_pickable_ids() {
    PICKABLE_NEXT.store(0, std::sync::atomic::Ordering::Relaxed);
}

static PICKABLE_NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        reset_pickable_ids();
        let signed_in = matches!(self.auth, AuthState::Authenticated { .. });

        // Signed out: only the login view, centred, with no app chrome.
        if !signed_in {
            return self.login_page(self.auth.clone(), cx);
        }

        let header = self.header();
        let account_row = self.account_row(cx);
        let content = match self.selected.clone() {
            Some(full_name) => self.workspace_shell(&full_name, cx),
            None => self.repository_picker(cx),
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
                    .child(content),
            )
            .child(self.status_bar(cx))
            .into_any_element()
    }
}
