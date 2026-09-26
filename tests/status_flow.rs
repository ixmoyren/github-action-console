use std::sync::Mutex;

use async_trait::async_trait;
use github_action_console::app::{
    AppProblem, MAX_NOTICES, Notice, NoticeKind, Status, notice_for, notice_from_gateway,
};
use github_action_console::github::{
    Account, BuildArtifact, DeviceFlowHandle, DeviceFlowPoll, DeviceFlowStart, FileContents,
    FileWrite, GatewayError, GitHubGateway, Job, PullRequest, RateLimit, ReleaseAsset,
    RepositoryPage, RepositorySort, SecretToken, Workflow, WorkflowRunPage,
};

#[derive(Default)]
struct FakeGateway {
    limits: Mutex<Option<Result<RateLimit, GatewayError>>>,
}

impl FakeGateway {
    fn with_limit(self, response: Result<RateLimit, GatewayError>) -> Self {
        *self.limits.lock().unwrap() = Some(response);
        self
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

    async fn file_contents(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _path: &str,
    ) -> Result<FileContents, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn runner_labels(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
    ) -> Result<Vec<String>, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn write_file(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _write: FileWrite,
    ) -> Result<(), GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn create_branch(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _branch: &str,
        _from: &str,
    ) -> Result<(), GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn open_pull_request(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _pull: PullRequest,
    ) -> Result<u64, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn dispatch_workflow(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _workflow: &str,
        _reference: &str,
        _inputs: &[(String, String)],
    ) -> Result<(), GatewayError> {
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
        Err(GatewayError::Unexpected("unused".to_owned()))
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
        _artifact_id: u64,
    ) -> Result<Vec<u8>, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn download_release_asset(
        &self,
        _token: &SecretToken,
        _url: &str,
    ) -> Result<Vec<u8>, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    fn set_proxy(&self, _proxy: Option<String>) {}

    async fn release_assets(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _tag: &str,
    ) -> Result<Vec<ReleaseAsset>, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn rate_limit(&self, _token: &SecretToken) -> Result<RateLimit, GatewayError> {
        self.limits
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Err(GatewayError::Unexpected(
                "no scripted limit".to_owned(),
            )))
    }
}

fn token() -> SecretToken {
    SecretToken::new("ghp_test_token")
}

fn notice(kind: NoticeKind, text: &str) -> Notice {
    Notice {
        kind,
        text: text.to_owned(),
    }
}

#[test]
fn every_problem_maps_to_a_notice_kind_and_copy() {
    assert_eq!(notice_for(AppProblem::Forbidden).kind, NoticeKind::Error);
    assert_eq!(notice_for(AppProblem::Network).kind, NoticeKind::Error);
    assert_eq!(notice_for(AppProblem::Unexpected).kind, NoticeKind::Error);
    assert_eq!(
        notice_for(AppProblem::RateLimited).kind,
        NoticeKind::Warning
    );
    assert_eq!(notice_for(AppProblem::NotFound).kind, NoticeKind::Warning);

    // Rate limiting is a warning with its own copy, not the generic failure.
    assert_ne!(
        notice_for(AppProblem::RateLimited).text,
        notice_for(AppProblem::Unexpected).text
    );
    for problem in [
        AppProblem::Forbidden,
        AppProblem::RateLimited,
        AppProblem::NotFound,
        AppProblem::Network,
        AppProblem::Unexpected,
    ] {
        assert!(!notice_for(problem).text.is_empty());
    }
}

#[test]
fn gateway_rate_limiting_maps_to_the_rate_limited_notice() {
    let notice = notice_from_gateway(&GatewayError::RateLimited);

    assert_eq!(notice, notice_for(AppProblem::RateLimited));
}

#[test]
fn gateway_transport_errors_map_to_the_network_notice() {
    let notice = notice_from_gateway(&GatewayError::Transport("offline".to_owned()));

    assert_eq!(notice, notice_for(AppProblem::Network));
}

#[test]
fn notices_are_capped_and_drop_the_oldest() {
    let mut status = Status::new();

    for index in 0..(MAX_NOTICES + 2) {
        status.push(notice(NoticeKind::Info, &format!("notice {index}")));
    }

    assert_eq!(status.notices().len(), MAX_NOTICES);
    assert_eq!(status.notices()[0].text, "notice 2");
}

#[test]
fn an_immediate_repeat_is_not_pushed_twice() {
    let mut status = Status::new();

    status.push_problem(AppProblem::Network);
    status.push_problem(AppProblem::Network);
    status.push_problem(AppProblem::Unexpected);
    status.push_problem(AppProblem::Network);

    assert_eq!(status.notices().len(), 3);
}

#[test]
fn dismissing_clears_every_notice() {
    let mut status = Status::new();
    status.push_problem(AppProblem::Network);

    status.dismiss_all();

    assert!(status.notices().is_empty());
}

#[tokio::test]
async fn a_successful_refresh_records_the_rate_limit_without_a_notice() {
    let gateway = FakeGateway::default().with_limit(Ok(RateLimit {
        limit: 5000,
        remaining: 4999,
        reset_at: Some("2026-09-22T11:00:00Z".to_owned()),
    }));
    let mut status = Status::new();

    status.refresh_rate_limit(&gateway, &token()).await;

    assert_eq!(status.remaining(), Some(4999));
    assert_eq!(status.reset_at(), Some("2026-09-22T11:00:00Z"));
    assert!(status.notices().is_empty());
}

#[tokio::test]
async fn a_failed_refresh_clears_the_budget_and_raises_a_notice() {
    let gateway =
        FakeGateway::default().with_limit(Err(GatewayError::Transport("offline".to_owned())));
    let mut status = Status::new();
    status.set_account(Some("octocat".to_owned()));

    status.refresh_rate_limit(&gateway, &token()).await;

    assert_eq!(status.remaining(), None);
    assert_eq!(status.notices(), &[notice_for(AppProblem::Network)]);
    assert_eq!(status.account(), Some("octocat"));
}
