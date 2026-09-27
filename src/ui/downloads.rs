use super::AppView;
use super::*;

use gpui_kit::component::progress::Progress;

impl AppView {
    pub(super) async fn refresh_downloads(
        downloads: &Arc<Mutex<Downloads>>,
        this: &WeakEntity<AppView>,
        cx: &mut AsyncApp,
    ) {
        let state = { downloads.lock().await.state().clone() };
        if let Err(error) = this.update(cx, |this, cx| {
            this.announce_download(&state);
            this.download_state = state;
            cx.notify();
        }) {
            warn!(?error, "the view was gone before the update landed");
        };
    }

    /// 下载落地了就说一声：弹一个提示，上面写着文件存到哪儿了（或者为什么没存下来）。
    ///
    /// 提示只是排在队里：弹出要 window，而这里在异步任务里，下一帧 render 会来取。
    pub(super) fn announce_download(&mut self, state: &DownloadState) {
        let notification = match download_announcement(state) {
            Some(DownloadAnnouncement::Saved(text)) => {
                Notification::success(text).title(labels::DOWNLOAD_DONE)
            }
            Some(DownloadAnnouncement::Failed(text)) => {
                Notification::error(text).title(labels::DOWNLOAD_FAILED)
            }
            None => return,
        };
        self.pending_notifications.push(notification);
    }

    pub(super) fn load_artifacts(&mut self, cx: &mut Context<Self>) {
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let detail = self.detail.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let (repository, run_id) = {
                let guard = detail.lock().await;
                (guard.repository().map(str::to_owned), guard.run_id())
            };
            let (Some(repository), Some(run_id)) = (repository, run_id) else {
                return;
            };
            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };

            let task = runtime.spawn({
                let gateway = gateway.clone();
                async move {
                    let (owner, name) = crate::github::split_full_name(&repository)?;
                    gateway
                        .list_artifacts(&token, &owner, &name, run_id)
                        .await
                        .ok()
                }
            });
            let artifacts = task.await.ok().flatten();
            if let Err(error) = this.update(cx, |this, cx| {
                match artifacts {
                    Some(artifacts) => {
                        this.artifacts = artifacts;
                        this.artifacts_state = LoadState::Loaded;
                    }
                    None => this.artifacts_state = LoadState::Failed(AppProblem::Unexpected),
                }
                cx.notify();
            }) {
                warn!(?error, "the view was gone before the update landed");
            };
        })
        .detach();
    }
    pub(super) fn download_run_logs(&mut self, cx: &mut Context<Self>) {
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let detail = self.detail.clone();
        let downloads = self.downloads.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let (repository, run_id) = {
                let guard = detail.lock().await;
                (guard.repository().map(str::to_owned), guard.run_id())
            };
            let (Some(repository), Some(run_id)) = (repository, run_id) else {
                return;
            };
            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };

            // 先把"正在下"露在界面上：状态栏那一句、行里的进度条和取消都要当场出现，
            // 不能等请求回来。
            let task = { downloads.lock().await.begin_run_logs(run_id) };
            Self::refresh_downloads(&downloads, &this, cx).await;

            let task = runtime.spawn({
                let gateway = gateway.clone();
                let downloads = downloads.clone();
                async move {
                    downloads
                        .lock()
                        .await
                        .download(&*gateway, &token, &repository, task)
                        .await;
                }
            });
            let abort = task.abort_handle();
            if let Err(error) = this.update(cx, |this, _| this.download_abort = Some(abort)) {
                warn!(?error, "the view was gone before the update landed");
            }
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
            if let Err(error) = this.update(cx, |this, _| this.download_abort = None) {
                warn!(?error, "the view was gone before the update landed");
            }
            Self::refresh_downloads(&downloads, &this, cx).await;
        })
        .detach();
    }
    pub(super) fn download_artifact(&mut self, artifact: BuildArtifact, cx: &mut Context<Self>) {
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        // 产物不止从运行详情页下：运行列表里点开「已完成」也走这里。仓库以当前
        // 工作区为准，那条路根本不用先打开详情。
        let workspace = self.workspace.clone();
        let downloads = self.downloads.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let repository = { workspace.lock().await.repository().map(str::to_owned) };
            let Some(repository) = repository else {
                return;
            };

            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };

            // 先把"正在下"露在界面上：状态栏那一句、行里的进度条和取消都要当场出现。
            let task = { downloads.lock().await.begin_artifact(&artifact) };
            Self::refresh_downloads(&downloads, &this, cx).await;

            let task = runtime.spawn({
                let gateway = gateway.clone();
                let downloads = downloads.clone();
                async move {
                    downloads
                        .lock()
                        .await
                        .download(&*gateway, &token, &repository, task)
                        .await;
                }
            });
            let abort = task.abort_handle();
            if let Err(error) = this.update(cx, |this, _| this.download_abort = Some(abort)) {
                warn!(?error, "the view was gone before the update landed");
            }
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
            if let Err(error) = this.update(cx, |this, _| this.download_abort = None) {
                warn!(?error, "the view was gone before the update landed");
            }
            Self::refresh_downloads(&downloads, &this, cx).await;
        })
        .detach();
    }
    /// 下载一个发布资产：和构建产物走同一条路。
    pub(super) fn download_release_asset(&mut self, asset: ReleaseAsset, cx: &mut Context<Self>) {
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let detail = self.detail.clone();
        let downloads = self.downloads.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let repository = { detail.lock().await.repository().map(str::to_owned) };
            let Some(repository) = repository else {
                return;
            };

            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };

            // 先把"正在下"露在界面上：没有下载地址的那种，这一步就把失败记下了。
            let task = { downloads.lock().await.begin_release_asset(&asset) };
            Self::refresh_downloads(&downloads, &this, cx).await;
            let Some(task) = task else {
                return;
            };

            let task = runtime.spawn({
                let gateway = gateway.clone();
                let downloads = downloads.clone();
                async move {
                    downloads
                        .lock()
                        .await
                        .download(&*gateway, &token, &repository, task)
                        .await;
                }
            });
            let abort = task.abort_handle();
            if let Err(error) = this.update(cx, |this, _| this.download_abort = Some(abort)) {
                warn!(?error, "the view was gone before the update landed");
            }
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
            if let Err(error) = this.update(cx, |this, _| this.download_abort = None) {
                warn!(?error, "the view was gone before the update landed");
            }
            Self::refresh_downloads(&downloads, &this, cx).await;
        })
        .detach();
    }

    /// 取消正在下的那个：把请求掐掉，状态收回"什么都没发生"。
    ///
    /// 文件是取完才写的，所以掐在中间不会留下半个文件。
    pub(super) fn cancel_download(&mut self, cx: &mut Context<Self>) {
        if let Some(abort) = self.download_abort.take() {
            abort.abort();
        }
        let downloads = self.downloads.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let downloads = downloads.clone();
                async move {
                    downloads.lock().await.cancel();
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
            Self::refresh_downloads(&downloads, &this, cx).await;
        })
        .detach();
    }
}

/// 下载落地后要说的那句话是哪一种：存下来了，还是没存下来。
///
/// 纯的，所以文案（尤其是那句带着路径的成功提示）能直接测，而不用真下个文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DownloadAnnouncement {
    Saved(String),
    Failed(String),
}

/// 下载落地后的那句话：成功报文件存到哪儿了，失败报原因，还在路上就没什么好说的。
pub(super) fn download_announcement(state: &DownloadState) -> Option<DownloadAnnouncement> {
    match state {
        DownloadState::Saved(task) => Some(DownloadAnnouncement::Saved(saved_text(&task.path))),
        DownloadState::Failed { problem, .. } => Some(DownloadAnnouncement::Failed(
            notice_for(*problem).text.to_owned(),
        )),
        DownloadState::Idle | DownloadState::Downloading(_) => None,
    }
}

/// "文件正在写入 <路径> 中"——状态栏在下载期间说的就是这一句。
pub(super) fn writing_text(path: &std::path::Path) -> String {
    format!("{} {} 中", labels::DOWNLOAD_WRITING, path.display())
}

/// "文件已经保存到 <路径> 中"——行里、提示上说的是同一句。
pub(super) fn saved_text(path: &std::path::Path) -> String {
    format!("{} {} 中", labels::DOWNLOAD_SAVED, path.display())
}

/// 一样东西的下载入口，按状态换样子：
///
/// - 没在下、也没下过：一颗下载按钮；
/// - 正在下（就是这一样）：进度条 + 取消；
/// - 下好了（就是这一样）：先说一句"文件已经保存到哪儿了"，后面再跟一颗下载按钮；
/// - 刚下失败（就是这一样）：先说为什么，后面再跟一颗下载按钮。
pub(super) fn download_controls(
    state: &DownloadState,
    kind: &DownloadKind,
    button_id: SharedString,
    label: &'static str,
    on_download: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &mut Context<AppView>,
) -> AnyElement {
    if matches!(state, DownloadState::Downloading(task) if task.kind == *kind) {
        return div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(download_progress(SharedString::from(format!(
                "{button_id}-progress"
            ))))
            .child(
                Button::new(SharedString::from(format!("{button_id}-cancel")))
                    .xsmall()
                    .label(labels::DOWNLOAD_CANCEL)
                    .on_click(cx.listener(|this, _, _, cx| this.cancel_download(cx))),
            )
            .into_any_element();
    }

    let mut controls = div().flex().flex_row().items_center().gap_2();
    match state {
        DownloadState::Saved(task) if task.kind == *kind => {
            controls = controls.child(Label::new(saved_text(&task.path)).text_sm());
        }
        DownloadState::Failed { task, problem } if task.kind == *kind => {
            controls = controls.child(Label::new(problem_text(*problem)).text_sm());
        }
        _ => {}
    }
    controls
        .child(
            Button::new(button_id)
                .xsmall()
                .label(label)
                .on_click(on_download),
        )
        .into_any_element()
}

/// 下载中的进度条。
///
/// 只能是不确定的那种：octocrab 一口气把文件读进内存，没有可以数的字节流，
/// 所以这里不谎报百分比。
pub(super) fn download_progress(id: SharedString) -> AnyElement {
    let bar = SharedString::from(format!("{id}-bar"));
    div()
        .id(id)
        .test_support()
        .w(px(140.))
        .child(
            Progress::new(bar)
                .loading(true)
                .with_size(gpui_kit::component::Size::Small)
                .accessibility_label(labels::DOWNLOADING)
                .w_full(),
        )
        .into_any_element()
}
