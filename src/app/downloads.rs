use std::path::PathBuf;

use tracing::{debug, info, warn};

use crate::github::{BuildArtifact, GitHubGateway, ReleaseAsset, SecretToken, split_full_name};

use super::repositories::AppProblem;

/// Artifacts larger than this need an explicit confirmation before download.
pub const DEFAULT_SIZE_THRESHOLD_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadKind {
    RunLogs {
        run_id: u64,
    },
    Artifact {
        artifact_id: u64,
    },
    /// 发布资产按下载地址取：它是 Release 上的文件，不是 Actions 的产物。
    ReleaseAsset {
        url: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDownload {
    pub file_name: String,
    pub size_in_bytes: Option<u64>,
    pub kind: DownloadKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DownloadState {
    #[default]
    Idle,
    NeedsConfirmation(PendingDownload),
    Downloading,
    Saved(PathBuf),
    Failed(AppProblem),
}

/// Downloads run log archives, build artifacts and release assets to a local
/// directory.
/// octocrab buffers whole payloads in memory, so anything large is confirmed
/// first (ticket 07).
pub struct Downloads {
    directory: PathBuf,
    threshold: u64,
    state: DownloadState,
}

impl Downloads {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self::with_threshold(directory, DEFAULT_SIZE_THRESHOLD_BYTES)
    }

    pub fn with_threshold(directory: impl Into<PathBuf>, threshold: u64) -> Self {
        Self {
            directory: directory.into(),
            threshold,
            state: DownloadState::Idle,
        }
    }

    pub fn state(&self) -> &DownloadState {
        &self.state
    }

    pub fn threshold(&self) -> u64 {
        self.threshold
    }

    pub fn file_name_for_artifact(artifact: &BuildArtifact) -> String {
        format!("{}.zip", sanitize(&artifact.name))
    }

    pub fn file_name_for_run_logs(run_id: u64) -> String {
        format!("run-{run_id}-logs.zip")
    }

    /// 发布资产用它在 Release 上的名字当文件名——那正是用户下载时要找的名字。
    pub fn file_name_for_release_asset(asset: &ReleaseAsset) -> String {
        sanitize(&asset.name)
    }

    /// Whether this artifact is big enough to need a confirmation.
    pub fn needs_confirmation(&self, artifact: &BuildArtifact) -> bool {
        artifact.size_in_bytes > self.threshold
    }

    /// Whether something this size is big enough to need a confirmation.
    pub fn needs_confirmation_for(&self, size_in_bytes: u64) -> bool {
        size_in_bytes > self.threshold
    }

    /// Park an artifact behind a confirmation prompt.
    pub fn queue_artifact(&mut self, artifact: &BuildArtifact) {
        self.state = DownloadState::NeedsConfirmation(PendingDownload {
            file_name: Self::file_name_for_artifact(artifact),
            size_in_bytes: Some(artifact.size_in_bytes),
            kind: DownloadKind::Artifact {
                artifact_id: artifact.id,
            },
        });
    }

    /// Park a release asset behind a confirmation prompt.
    pub fn queue_release_asset(&mut self, asset: &ReleaseAsset) {
        let Some(url) = asset.download_url.clone() else {
            return;
        };
        self.state = DownloadState::NeedsConfirmation(PendingDownload {
            file_name: Self::file_name_for_release_asset(asset),
            size_in_bytes: Some(asset.size_in_bytes),
            kind: DownloadKind::ReleaseAsset { url },
        });
    }

    pub fn cancel(&mut self) {
        self.state = DownloadState::Idle;
    }

    pub async fn download_run_logs(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        repository: &str,
        run_id: u64,
    ) {
        let file_name = Self::file_name_for_run_logs(run_id);
        self.perform(
            gateway,
            token,
            repository,
            DownloadKind::RunLogs { run_id },
            file_name,
        )
        .await;
    }

    pub async fn download_artifact(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        repository: &str,
        artifact: &BuildArtifact,
    ) {
        let file_name = Self::file_name_for_artifact(artifact);
        self.perform(
            gateway,
            token,
            repository,
            DownloadKind::Artifact {
                artifact_id: artifact.id,
            },
            file_name,
        )
        .await;
    }

    /// 下载一个发布资产：URL 缺失就没什么可下的，直接当作失败。
    pub async fn download_release_asset(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        repository: &str,
        asset: &ReleaseAsset,
    ) {
        let Some(url) = asset.download_url.clone() else {
            self.state = DownloadState::Failed(AppProblem::Unexpected);
            return;
        };
        let file_name = Self::file_name_for_release_asset(asset);
        self.perform(
            gateway,
            token,
            repository,
            DownloadKind::ReleaseAsset { url },
            file_name,
        )
        .await;
    }

    /// Run the download the user just confirmed.
    pub async fn confirm(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        repository: &str,
    ) {
        let DownloadState::NeedsConfirmation(pending) = self.state.clone() else {
            return;
        };
        self.perform(gateway, token, repository, pending.kind, pending.file_name)
            .await;
    }

    /// The artifact behind the current confirmation prompt, if any.
    pub fn pending(&self) -> Option<&PendingDownload> {
        match &self.state {
            DownloadState::NeedsConfirmation(pending) => Some(pending),
            _ => None,
        }
    }

    async fn perform(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        repository: &str,
        kind: DownloadKind,
        file_name: String,
    ) {
        let Some((owner, repository_name)) = split_full_name(repository) else {
            self.state = DownloadState::Failed(AppProblem::Unexpected);
            return;
        };

        debug!(file = %file_name, "downloading");
        self.state = DownloadState::Downloading;
        let bytes = match kind {
            DownloadKind::RunLogs { run_id } => {
                gateway
                    .run_logs_archive(token, &owner, &repository_name, run_id)
                    .await
            }
            DownloadKind::Artifact { artifact_id } => {
                gateway
                    .download_artifact(token, &owner, &repository_name, artifact_id)
                    .await
            }
            DownloadKind::ReleaseAsset { url } => gateway.download_release_asset(token, &url).await,
        };

        match bytes {
            Ok(bytes) => match self.save(&file_name, &bytes) {
                Ok(path) => {
                    info!(path = %path.display(), bytes = bytes.len(), "saved a download");
                    self.state = DownloadState::Saved(path);
                }
                Err(error) => {
                    warn!(%error, file = %file_name, "could not write the download");
                    self.state = DownloadState::Failed(AppProblem::Unexpected);
                }
            },
            Err(error) => {
                warn!(%error, file = %file_name, "download failed");
                self.state = DownloadState::Failed(AppProblem::from_gateway(&error));
            }
        }
    }

    fn save(&self, file_name: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&self.directory)?;
        let path = self.directory.join(file_name);
        std::fs::write(&path, bytes)?;
        Ok(path)
    }

    /// Write a file the console produced itself — a workflow draft, say — into
    /// the same local directory artifacts go to. An existing file of the same
    /// name is replaced.
    pub fn save_file(&self, file_name: &str, contents: &str) -> std::io::Result<PathBuf> {
        self.save(&sanitize(file_name), contents.as_bytes())
    }
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}
