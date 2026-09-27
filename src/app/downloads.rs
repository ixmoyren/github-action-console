use std::path::PathBuf;

use tracing::{debug, info, warn};

use crate::github::{BuildArtifact, GitHubGateway, ReleaseAsset, SecretToken, split_full_name};

use super::repositories::AppProblem;

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
/// 一次下载：下的是哪一样东西、文件叫什么、多大。
///
/// 正在下的那一个会一直挂在状态里，界面才知道把进度条摆在哪一行。
pub struct DownloadTask {
    /// 文件写到哪儿——状态栏和行里说"正在写入 / 已经保存到"时用的就是它。
    pub path: PathBuf,
    pub size_in_bytes: Option<u64>,
    pub kind: DownloadKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DownloadState {
    #[default]
    Idle,
    Downloading(DownloadTask),
    /// 下好了：哪一样东西，存到哪儿了。
    Saved(DownloadTask),
    /// 没下下来：哪一样东西，为什么。
    Failed {
        task: DownloadTask,
        problem: AppProblem,
    },
}

/// Downloads run log archives, build artifacts and release assets to a local
/// directory.
///
/// 点下载就是下，不再先问一句——大文件也一样。进度只能是"不确定"的那种：
/// octocrab 一口气把整个文件读进内存，没有可以数的字节流（ticket 07）。
pub struct Downloads {
    directory: PathBuf,
    state: DownloadState,
}

impl Downloads {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            state: DownloadState::Idle,
        }
    }

    pub fn state(&self) -> &DownloadState {
        &self.state
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

    /// 记下"这一样东西开始下了"，并把这件事交给调用方。
    ///
    /// 界面要用这个任务当场把进度条和状态栏那句话摆出来——不能等请求回来才动。
    pub fn begin_run_logs(&mut self, run_id: u64) -> DownloadTask {
        let task = DownloadTask {
            path: self.directory.join(Self::file_name_for_run_logs(run_id)),
            size_in_bytes: None,
            kind: DownloadKind::RunLogs { run_id },
        };
        self.state = DownloadState::Downloading(task.clone());
        task
    }

    pub fn begin_artifact(&mut self, artifact: &BuildArtifact) -> DownloadTask {
        let task = DownloadTask {
            path: self.directory.join(Self::file_name_for_artifact(artifact)),
            size_in_bytes: Some(artifact.size_in_bytes),
            kind: DownloadKind::Artifact {
                artifact_id: artifact.id,
            },
        };
        self.state = DownloadState::Downloading(task.clone());
        task
    }

    /// 发布资产没有下载地址就没什么可下的：当场记成失败，也没有任务可下。
    pub fn begin_release_asset(&mut self, asset: &ReleaseAsset) -> Option<DownloadTask> {
        let task = DownloadTask {
            path: self
                .directory
                .join(Self::file_name_for_release_asset(asset)),
            size_in_bytes: Some(asset.size_in_bytes),
            kind: DownloadKind::ReleaseAsset {
                url: asset.download_url.clone().unwrap_or_default(),
            },
        };
        match asset.download_url {
            Some(_) => {
                self.state = DownloadState::Downloading(task.clone());
                Some(task)
            }
            None => {
                self.state = DownloadState::Failed {
                    task,
                    problem: AppProblem::Unexpected,
                };
                None
            }
        }
    }

    /// 取消：把状态收回"什么都没发生"。
    ///
    /// 真正掐掉请求的是界面那边的任务把手；这里只负责别让状态停在"正在下"。
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
        let task = self.begin_run_logs(run_id);
        self.download(gateway, token, repository, task).await;
    }

    pub async fn download_artifact(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        repository: &str,
        artifact: &BuildArtifact,
    ) {
        let task = self.begin_artifact(artifact);
        self.download(gateway, token, repository, task).await;
    }

    /// 下载一个发布资产：URL 缺失就没什么可下的，直接当作失败。
    pub async fn download_release_asset(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        repository: &str,
        asset: &ReleaseAsset,
    ) {
        let Some(task) = self.begin_release_asset(asset) else {
            return;
        };
        self.download(gateway, token, repository, task).await;
    }

    /// 取回一个已经 `begin_*` 过的下载，写进它自己那个路径。
    pub async fn download(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        repository: &str,
        task: DownloadTask,
    ) {
        let Some((owner, repository_name)) = split_full_name(repository) else {
            self.state = DownloadState::Failed {
                task,
                problem: AppProblem::Unexpected,
            };
            return;
        };

        let kind = task.kind.clone();
        debug!(path = %task.path.display(), "downloading");
        self.state = DownloadState::Downloading(task.clone());
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
            Ok(bytes) => match self.save(&task.path, &bytes) {
                Ok(()) => {
                    info!(path = %task.path.display(), bytes = bytes.len(), "saved a download");
                    self.state = DownloadState::Saved(task);
                }
                Err(error) => {
                    warn!(%error, path = %task.path.display(), "could not write the download");
                    self.state = DownloadState::Failed {
                        task,
                        problem: AppProblem::Unexpected,
                    };
                }
            },
            Err(error) => {
                warn!(%error, "download failed");
                self.state = DownloadState::Failed {
                    task,
                    problem: AppProblem::from_gateway(&error),
                };
            }
        }
    }

    fn save(&self, path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.directory)?;
        std::fs::write(path, bytes)
    }

    /// Write a file the console produced itself — a workflow draft, say — into
    /// the same local directory artifacts go to. An existing file of the same
    /// name is replaced.
    pub fn save_file(&self, file_name: &str, contents: &str) -> std::io::Result<PathBuf> {
        let path = self.directory.join(sanitize(file_name));
        self.save(&path, contents.as_bytes())?;
        Ok(path)
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
