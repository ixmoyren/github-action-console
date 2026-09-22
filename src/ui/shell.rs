use super::AppView;
use super::*;

pub(crate) fn notice_text(problem: AuthProblem) -> &'static str {
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
        .child(Label::new(value.to_owned()))
}

impl AppView {
    pub(super) fn header(&self) -> impl IntoElement {
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
            .child(self.status_bar(cx))
    }
}
