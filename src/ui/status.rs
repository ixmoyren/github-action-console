use super::AppView;

use gpui_kit::component::Sizable as _;
use gpui_kit::component::status_bar::StatusBar;

use super::*;

use super::downloads::writing_text;

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
            DownloadState::Failed { problem, .. } => Some(problem),
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

        if let Some(location) = self.workspace_location() {
            bar = bar.left(location);
        }

        if let Some(problem) = self.current_failure() {
            bar = bar.child(Label::new(crate::app::notice_for(problem).text).text_sm());
        }
        // 正在写盘的时候，状态栏里说一句写到哪个文件了。
        if let DownloadState::Downloading(task) = &self.download_state {
            bar = bar.child(
                div()
                    .id("download-writing")
                    .test_support()
                    .child(Label::new(writing_text(&task.path)).text_sm()),
            );
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

    /// The selected repository's branch, short commit, and blame line, shown at
    /// the left of the status bar while a workspace is open.
    fn workspace_location(&self) -> Option<String> {
        let full_name = self.selected.as_deref()?;
        let repository = self
            .repos
            .iter()
            .find(|repository| repository.full_name == full_name)?;

        let branch = repository
            .default_branch
            .clone()
            .unwrap_or_else(|| labels::VALUE_MISSING.to_owned());
        let commit = repository.latest_commit.as_ref();
        let sha = commit
            .map(|commit| short_sha(&commit.sha))
            .unwrap_or_else(|| labels::VALUE_MISSING.to_owned());
        let blame = commit
            .map(blame_text)
            .unwrap_or_else(|| labels::VALUE_MISSING.to_owned());

        Some(format!("{branch} · {sha} · {blame}"))
    }
}

fn short_sha(sha: &str) -> String {
    sha.chars().take(7).collect()
}

fn blame_text(commit: &CommitSummary) -> String {
    let author = commit
        .author
        .clone()
        .unwrap_or_else(|| labels::VALUE_MISSING.to_owned());
    match commit.committed_at.as_deref() {
        Some(date) => format!("{author} {}", super::repositories::format_commit_date(date)),
        None => author,
    }
}
