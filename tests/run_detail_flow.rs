use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use github_action_console::app::{AppProblem, LoadState, RunDetail};
use github_action_console::github::{
    Account, DeviceFlowHandle, DeviceFlowPoll, DeviceFlowStart, FileContents, FileWrite,
    GatewayError, GitHubGateway, Job, RepositoryPage, RepositorySort, RunStatus, SecretToken, Step,
    Workflow, WorkflowRun, WorkflowRunPage, filter_log_lines,
};

#[derive(Default)]
struct FakeGateway {
    jobs: Mutex<VecDeque<Result<Vec<Job>, GatewayError>>>,
    logs: Mutex<VecDeque<Result<String, GatewayError>>>,
    requests: Mutex<Vec<(u64, u64)>>,
}

impl FakeGateway {
    fn push_jobs(&self, response: Result<Vec<Job>, GatewayError>) {
        self.jobs.lock().unwrap().push_back(response);
    }

    fn push_logs(&self, response: Result<String, GatewayError>) {
        self.logs.lock().unwrap().push_back(response);
    }

    fn requests(&self) -> Vec<(u64, u64)> {
        self.requests.lock().unwrap().clone()
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

    async fn write_file(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _write: FileWrite,
    ) -> Result<(), GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn dispatch_workflow(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _workflow_id: u64,
        _reference: &str,
    ) -> Result<(), GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn list_jobs(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        run_id: u64,
    ) -> Result<Vec<Job>, GatewayError> {
        self.requests.lock().unwrap().push((run_id, 0));
        self.jobs
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected("no scripted jobs".to_owned())))
    }

    async fn job_logs(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        job_id: u64,
    ) -> Result<String, GatewayError> {
        self.requests.lock().unwrap().push((0, job_id));
        self.logs
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected("no scripted logs".to_owned())))
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
    ) -> Result<Vec<github_action_console::github::BuildArtifact>, GatewayError> {
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

    fn set_proxy(&self, _proxy: Option<String>) {}

    async fn rate_limit(
        &self,
        _token: &SecretToken,
    ) -> Result<github_action_console::github::RateLimit, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }
}

fn step(number: i64, name: &str, status: RunStatus, conclusion: Option<&str>) -> Step {
    Step {
        number,
        name: name.to_owned(),
        status,
        conclusion: conclusion.map(str::to_owned),
    }
}

fn job(id: u64, name: &str, status: RunStatus, conclusion: Option<&str>) -> Job {
    Job {
        id,
        name: name.to_owned(),
        status,
        conclusion: conclusion.map(str::to_owned),
        steps: vec![
            step(1, "Set up job", RunStatus::Completed, Some("success")),
            step(2, "Run tests", RunStatus::Completed, conclusion),
        ],
    }
}

fn run(id: u64, html_url: &str) -> WorkflowRun {
    WorkflowRun {
        id,
        workflow_id: 1,
        name: format!("run {id}"),
        status: RunStatus::Completed,
        conclusion: Some("success".to_owned()),
        branch: Some("main".to_owned()),
        event: "push".to_owned(),
        actor: Some("octocat".to_owned()),
        created_at: Some("2026-09-22T10:00:00Z".to_owned()),
        html_url: Some(html_url.to_owned()),
    }
}

fn token() -> SecretToken {
    SecretToken::new("ghp_test_token")
}

fn opened(gateway: &Arc<FakeGateway>, run_id: u64) -> RunDetail {
    let _ = gateway;
    let mut detail = RunDetail::new();
    detail.open(
        "octo/alpha",
        &run(run_id, "https://github.com/octo/alpha/actions/runs/10"),
    );
    detail
}

#[tokio::test]
async fn jobs_and_steps_are_loaded_with_their_statuses() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_jobs(Ok(vec![
        job(100, "build", RunStatus::Completed, Some("success")),
        job(101, "test", RunStatus::Completed, Some("success")),
    ]));
    let mut detail = opened(&gateway, 10);

    detail.load_jobs(&*gateway, &token()).await;

    assert_eq!(detail.state(), LoadState::Loaded);
    assert_eq!(detail.jobs().len(), 2);
    assert_eq!(detail.jobs()[0].name, "build");
    assert_eq!(detail.jobs()[0].steps.len(), 2);
    assert_eq!(detail.jobs()[0].steps[1].name, "Run tests");
    assert_eq!(gateway.requests(), vec![(10, 0)]);
}

#[tokio::test]
async fn a_failed_job_keeps_its_failure_conclusion() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_jobs(Ok(vec![job(
        100,
        "test",
        RunStatus::Completed,
        Some("failure"),
    )]));
    let mut detail = opened(&gateway, 10);

    detail.load_jobs(&*gateway, &token()).await;

    assert_eq!(detail.jobs()[0].conclusion.as_deref(), Some("failure"));
    assert_eq!(
        detail.jobs()[0].steps[1].conclusion.as_deref(),
        Some("failure")
    );
}

#[tokio::test]
async fn logs_are_read_for_the_selected_job() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_logs(Ok("line one\nline two\n".to_owned()));
    let mut detail = opened(&gateway, 10);

    detail.select_job(100);
    detail.load_logs(&*gateway, &token()).await;

    assert_eq!(detail.logs_state(), LoadState::Loaded);
    assert_eq!(detail.logs(), Some("line one\nline two\n"));
    assert_eq!(gateway.requests(), vec![(0, 100)]);
}

#[tokio::test]
async fn an_empty_log_is_a_loaded_empty_result() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_logs(Ok(String::new()));
    let mut detail = opened(&gateway, 10);

    detail.select_job(100);
    detail.load_logs(&*gateway, &token()).await;

    assert_eq!(detail.logs_state(), LoadState::Loaded);
    assert_eq!(detail.logs(), Some(""));
}

#[tokio::test]
async fn a_log_failure_is_surfaced() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_logs(Err(GatewayError::Transport("offline".to_owned())));
    let mut detail = opened(&gateway, 10);

    detail.select_job(100);
    detail.load_logs(&*gateway, &token()).await;

    assert_eq!(detail.logs_state(), LoadState::Failed(AppProblem::Network));
}

#[tokio::test]
async fn selecting_another_job_discards_the_previous_log() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_logs(Ok("first job".to_owned()));
    let mut detail = opened(&gateway, 10);

    detail.select_job(100);
    detail.load_logs(&*gateway, &token()).await;
    assert_eq!(detail.logs(), Some("first job"));

    detail.select_job(101);

    assert_eq!(detail.selected_job(), Some(101));
    assert_eq!(detail.logs(), None);
    assert_eq!(detail.logs_state(), LoadState::Idle);
}

#[tokio::test]
async fn the_run_page_url_is_available_for_opening_in_a_browser() {
    let gateway = Arc::new(FakeGateway::default());
    let detail = opened(&gateway, 10);

    assert_eq!(
        detail.html_url(),
        Some("https://github.com/octo/alpha/actions/runs/10")
    );
}

#[test]
fn log_lines_can_be_filtered_case_insensitively() {
    let log = "Compiling foo\nwarning: unused\nERROR: boom\n";

    assert_eq!(filter_log_lines(log, "error"), "ERROR: boom");
    assert_eq!(filter_log_lines(log, "WARN"), "warning: unused");
    assert_eq!(filter_log_lines(log, "not-there"), "");
}

#[test]
fn an_empty_log_query_returns_the_whole_log() {
    let log = "alpha\nbeta\n";

    assert_eq!(filter_log_lines(log, ""), log);
    assert_eq!(filter_log_lines(log, "   "), log);
}
