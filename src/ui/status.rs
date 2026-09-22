use super::AppView;

use gpui_kit::component::Sizable as _;
use gpui_kit::component::status_bar::StatusBar;

use super::*;

impl AppView {
    pub(super) async fn refresh_status(
        status: &Arc<Mutex<Status>>,
        this: &WeakEntity<AppView>,
        cx: &mut AsyncApp,
    ) {
        let guard = status.lock().await;
        let account = guard.account().map(str::to_owned);
        let remaining = guard.remaining();
        let reset_at = guard.reset_at().map(str::to_owned);
        let notices = guard.notices().to_vec();
        drop(guard);

        if let Err(error) = this.update(cx, |this, cx| {
            this.status_account = account;
            this.status_remaining = remaining;
            this.status_reset_at = reset_at;
            this.notices = notices;
            cx.notify();
        }) {
            warn!(?error, "the view was gone before the update landed");
        };
    }
    pub(super) fn refresh_status_bar(&mut self, login: Option<String>, cx: &mut Context<Self>) {
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let status = self.status.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let token = { manager.lock().await.token() };
            let task = runtime.spawn({
                let gateway = gateway.clone();
                let status = status.clone();
                let token = token.clone();
                async move {
                    let mut guard = status.lock().await;
                    guard.set_account(login);
                    if let Some(token) = token {
                        guard.refresh_rate_limit(&*gateway, &token).await;
                    }
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
            Self::refresh_status(&status, &this, cx).await;
        })
        .detach();
    }
    pub(super) fn dismiss_notices(&mut self, cx: &mut Context<Self>) {
        let status = self.status.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let status = status.clone();
                async move {
                    status.lock().await.dismiss_all();
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
            Self::refresh_status(&status, &this, cx).await;
        })
        .detach();
    }
    pub(super) fn current_failure(&self) -> Option<AppProblem> {
        if let RepositoryListState::Failed(problem) = self.repo_state {
            return Some(problem);
        }
        let states = [
            self.workflows_state,
            self.runs_state,
            self.jobs_state,
            self.logs_state,
            self.artifacts_state,
        ];
        for state in states {
            if let LoadState::Failed(problem) = state {
                return Some(problem);
            }
        }
        match self.download_state {
            DownloadState::Failed(problem) => Some(problem),
            _ => None,
        }
    }
    pub(super) fn status_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let rate = match (self.status_remaining, self.status_reset_at.as_deref()) {
            (Some(remaining), Some(reset)) => {
                format!(
                    "{}：{}（{}）",
                    labels::STATUS_BAR_RATE_LIMIT,
                    remaining,
                    reset
                )
            }
            (Some(remaining), None) => format!("{}：{}", labels::STATUS_BAR_RATE_LIMIT, remaining),
            _ => format!(
                "{}：{}",
                labels::STATUS_BAR_RATE_LIMIT,
                labels::VALUE_MISSING
            ),
        };

        let mut bar = StatusBar::new().right(rate);

        if let Some(problem) = self.current_failure() {
            bar = bar.child(Label::new(crate::app::notice_for(problem).text).text_sm());
        }
        for notice in &self.notices {
            bar = bar.child(Label::new(notice.text.clone()).text_sm());
        }
        if !self.notices.is_empty() {
            bar = bar.child(
                Button::new("dismiss-notices")
                    .ghost()
                    .xsmall()
                    .label(labels::NOTICES_DISMISS)
                    .on_click(cx.listener(|this, _, _, cx| this.dismiss_notices(cx))),
            );
        }

        bar.into_any_element()
    }
}
