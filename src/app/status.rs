use crate::github::{GatewayError, GitHubGateway, RateLimit, SecretToken};
use crate::labels;

use super::repositories::AppProblem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
}

/// How many notices are kept before the oldest are dropped.
pub const MAX_NOTICES: usize = 5;

/// The single mapping from a problem to the notice shown to the user. Pure, so
/// the copy is testable without a UI (ticket 08).
pub fn notice_for(problem: AppProblem) -> Notice {
    let (kind, text) = match problem {
        AppProblem::Forbidden => (NoticeKind::Error, labels::NOTICE_MISSING_SCOPES),
        AppProblem::RateLimited => (NoticeKind::Warning, labels::PROBLEM_RATE_LIMITED),
        AppProblem::NotFound => (NoticeKind::Warning, labels::PROBLEM_NOT_FOUND),
        AppProblem::Network => (NoticeKind::Error, labels::NOTICE_NETWORK),
        AppProblem::Unexpected => (NoticeKind::Error, labels::NOTICE_UNEXPECTED),
    };

    Notice {
        kind,
        text: text.to_owned(),
    }
}

pub fn notice_from_gateway(error: &GatewayError) -> Notice {
    notice_for(AppProblem::from_gateway(error))
}

/// The persistent status bar plus the transient notices shown beside it.
#[derive(Default)]
pub struct Status {
    account: Option<String>,
    rate_limit: Option<RateLimit>,
    notices: Vec<Notice>,
}

impl Status {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_account(&mut self, account: Option<String>) {
        self.account = account;
    }

    pub fn account(&self) -> Option<&str> {
        self.account.as_deref()
    }

    pub fn rate_limit(&self) -> Option<&RateLimit> {
        self.rate_limit.as_ref()
    }

    pub fn remaining(&self) -> Option<u64> {
        self.rate_limit.as_ref().map(|limit| limit.remaining)
    }

    pub fn reset_at(&self) -> Option<&str> {
        self.rate_limit
            .as_ref()
            .and_then(|limit| limit.reset_at.as_deref())
    }

    pub fn notices(&self) -> &[Notice] {
        &self.notices
    }

    pub fn dismiss_all(&mut self) {
        self.notices.clear();
    }

    /// Push a notice. The cap keeps the list bounded, and an immediate repeat
    /// of the newest notice is dropped so a poll loop cannot spam the bar.
    pub fn push(&mut self, notice: Notice) {
        if self.notices.last() == Some(&notice) {
            return;
        }
        self.notices.push(notice);
        while self.notices.len() > MAX_NOTICES {
            self.notices.remove(0);
        }
    }

    pub fn push_problem(&mut self, problem: AppProblem) {
        self.push(notice_for(problem));
    }

    pub async fn refresh_rate_limit(&mut self, gateway: &dyn GitHubGateway, token: &SecretToken) {
        match gateway.rate_limit(token).await {
            Ok(limit) => self.rate_limit = Some(limit),
            Err(error) => {
                self.rate_limit = None;
                self.push(notice_from_gateway(&error));
            }
        }
    }
}
