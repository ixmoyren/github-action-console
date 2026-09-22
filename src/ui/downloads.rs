use super::AppView;
use super::*;

impl AppView {
    pub(super) async fn refresh_downloads(
        downloads: &Arc<Mutex<Downloads>>,
        this: &WeakEntity<AppView>,
        cx: &mut AsyncApp,
    ) {
        let state = { downloads.lock().await.state().clone() };
        if let Err(error) = this.update(cx, |this, cx| {
            this.download_state = state;
            cx.notify();
        }) {
            warn!(?error, "the view was gone before the update landed");
        };
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

            let task = runtime.spawn({
                let gateway = gateway.clone();
                let downloads = downloads.clone();
                async move {
                    downloads
                        .lock()
                        .await
                        .download_run_logs(&*gateway, &token, &repository, run_id)
                        .await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
            Self::refresh_downloads(&downloads, &this, cx).await;
        })
        .detach();
    }
    pub(super) fn download_artifact(&mut self, artifact: BuildArtifact, cx: &mut Context<Self>) {
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

            // Large artifacts stop here and wait for a confirmation.
            let needs_confirmation = {
                let mut guard = downloads.lock().await;
                if guard.needs_confirmation(&artifact) {
                    guard.queue_artifact(&artifact);
                    true
                } else {
                    false
                }
            };
            if needs_confirmation {
                Self::refresh_downloads(&downloads, &this, cx).await;
                return;
            }

            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };
            let task = runtime.spawn({
                let gateway = gateway.clone();
                let downloads = downloads.clone();
                async move {
                    downloads
                        .lock()
                        .await
                        .download_artifact(&*gateway, &token, &repository, &artifact)
                        .await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
            Self::refresh_downloads(&downloads, &this, cx).await;
        })
        .detach();
    }
    pub(super) fn confirm_download(&mut self, cx: &mut Context<Self>) {
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

            let task = runtime.spawn({
                let gateway = gateway.clone();
                let downloads = downloads.clone();
                async move {
                    downloads
                        .lock()
                        .await
                        .confirm(&*gateway, &token, &repository)
                        .await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
            Self::refresh_downloads(&downloads, &this, cx).await;
        })
        .detach();
    }
    pub(super) fn cancel_download(&mut self, cx: &mut Context<Self>) {
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
