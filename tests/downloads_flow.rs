use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use github_action_console::app::{AppProblem, DownloadState, Downloads};
use github_action_console::github::{
    Account, BuildArtifact, DeviceFlowHandle, DeviceFlowPoll, DeviceFlowStart, GatewayError,
    GitHubGateway, Job, RepositoryPage, RepositorySort, SecretToken, Workflow, WorkflowRunPage,
};

#[derive(Default)]
struct FakeGateway {
    logs: Mutex<VecDeque<Result<Vec<u8>, GatewayError>>>,
    artifacts: Mutex<VecDeque<Result<Vec<u8>, GatewayError>>>,
    downloads: Mutex<Vec<u64>>,
}

impl FakeGateway {
    fn push_logs(&self, response: Result<Vec<u8>, GatewayError>) {
        self.logs.lock().unwrap().push_back(response);
    }

    fn push_artifact(&self, response: Result<Vec<u8>, GatewayError>) {
        self.artifacts.lock().unwrap().push_back(response);
    }

    fn downloads(&self) -> Vec<u64> {
        self.downloads.lock().unwrap().clone()
    }
}

#[async_trait]
impl GitHubGateway for FakeGateway {
    async fn start_device_flow(&self, _client_id: &str) -> Result<DeviceFlowStart, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn poll_device_flow(
        &self,
        _client_id: &str,
        _handle: &DeviceFlowHandle,
    ) -> Result<DeviceFlowPoll, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn current_user(&self, _token: &SecretToken) -> Result<Account, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn list_repositories(
        &self,
        _token: &SecretToken,
        _sort: RepositorySort,
        _page: u32,
        _per_page: u32,
    ) -> Result<RepositoryPage, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn list_workflows(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
    ) -> Result<Vec<Workflow>, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn list_workflow_runs(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _workflow_id: Option<u64>,
        _page: u32,
        _per_page: u32,
    ) -> Result<WorkflowRunPage, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn list_jobs(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _run_id: u64,
    ) -> Result<Vec<Job>, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn job_logs(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _job_id: u64,
    ) -> Result<String, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn run_logs_archive(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _run_id: u64,
    ) -> Result<Vec<u8>, GatewayError> {
        self.logs
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected("no scripted logs".to_owned())))
    }

    async fn list_artifacts(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _run_id: u64,
    ) -> Result<Vec<BuildArtifact>, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn download_artifact(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        artifact_id: u64,
    ) -> Result<Vec<u8>, GatewayError> {
        self.downloads.lock().unwrap().push(artifact_id);
        self.artifacts
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected(
                "no scripted artifact".to_owned(),
            )))
    }

    async fn rate_limit(
        &self,
        _token: &SecretToken,
    ) -> Result<github_action_console::github::RateLimit, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }
}

fn artifact(id: u64, name: &str, size_in_bytes: u64) -> BuildArtifact {
    BuildArtifact {
        id,
        name: name.to_owned(),
        size_in_bytes,
        expired: false,
        download_url: Some(format!("https://api.github.com/artifacts/{id}/zip")),
    }
}

fn token() -> SecretToken {
    SecretToken::new("ghp_test_token")
}

/// A unique scratch directory per test, so parallel tests never collide.
fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gac-download-test-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[tokio::test]
async fn a_small_artifact_downloads_without_confirmation_and_hits_disk() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_artifact(Ok(b"zip-bytes".to_vec()));
    let dir = scratch_dir("small");
    let mut downloads = Downloads::with_threshold(&dir, 1024);

    let artifact = artifact(7, "app.zip", 512);
    assert!(!downloads.needs_confirmation(&artifact));
    downloads
        .download_artifact(&*gateway, &token(), "octo/alpha", &artifact)
        .await;

    let saved = match downloads.state() {
        DownloadState::Saved(path) => path.clone(),
        other => panic!("expected a saved file, got {other:?}"),
    };
    assert_eq!(saved.file_name().unwrap(), "app.zip.zip");
    assert_eq!(std::fs::read(&saved).unwrap(), b"zip-bytes");
    assert_eq!(gateway.downloads(), vec![7]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_large_artifact_waits_for_confirmation_before_downloading() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_artifact(Ok(b"big".to_vec()));
    let dir = scratch_dir("large");
    let mut downloads = Downloads::with_threshold(&dir, 1024);

    let artifact = artifact(9, "installer.zip", 4096);
    assert!(downloads.needs_confirmation(&artifact));

    downloads.queue_artifact(&artifact);
    assert!(downloads.pending().is_some());
    // Nothing has been fetched yet.
    assert!(gateway.downloads().is_empty());

    downloads.confirm(&*gateway, &token(), "octo/alpha").await;

    assert!(matches!(downloads.state(), DownloadState::Saved(_)));
    assert_eq!(gateway.downloads(), vec![9]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn cancelling_a_confirmation_downloads_nothing() {
    let gateway = Arc::new(FakeGateway::default());
    let dir = scratch_dir("cancel");
    let mut downloads = Downloads::with_threshold(&dir, 1024);

    downloads.queue_artifact(&artifact(9, "installer.zip", 4096));
    downloads.cancel();

    assert_eq!(*downloads.state(), DownloadState::Idle);
    assert!(gateway.downloads().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_failed_artifact_download_is_surfaced() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_artifact(Err(GatewayError::Transport("offline".to_owned())));
    let dir = scratch_dir("artifact-fail");
    let mut downloads = Downloads::with_threshold(&dir, 1024);

    downloads
        .download_artifact(
            &*gateway,
            &token(),
            "octo/alpha",
            &artifact(7, "app.zip", 512),
        )
        .await;

    assert_eq!(
        *downloads.state(),
        DownloadState::Failed(AppProblem::Network)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn the_run_logs_archive_downloads_without_a_size_gate() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_logs(Ok(b"log-zip".to_vec()));
    let dir = scratch_dir("logs");
    let mut downloads = Downloads::with_threshold(&dir, 1);

    downloads
        .download_run_logs(&*gateway, &token(), "octo/alpha", 42)
        .await;

    let saved = match downloads.state() {
        DownloadState::Saved(path) => path.clone(),
        other => panic!("expected a saved file, got {other:?}"),
    };
    assert_eq!(saved.file_name().unwrap(), "run-42-logs.zip");
    assert_eq!(std::fs::read(&saved).unwrap(), b"log-zip");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_failed_run_logs_download_is_surfaced() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_logs(Err(GatewayError::NotFound));
    let dir = scratch_dir("logs-fail");
    let mut downloads = Downloads::with_threshold(&dir, 1024);

    downloads
        .download_run_logs(&*gateway, &token(), "octo/alpha", 42)
        .await;

    assert_eq!(
        *downloads.state(),
        DownloadState::Failed(AppProblem::NotFound)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn artifact_file_names_are_sanitised() {
    let artifact = artifact(1, "release notes/app v1.zip", 10);
    assert_eq!(
        Downloads::file_name_for_artifact(&artifact),
        "release_notes_app_v1.zip.zip"
    );
}
