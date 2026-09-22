use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use github_action_console::app::{LoadState, Workspace, WorkspaceTab};
use github_action_console::github::{
    Account, DeviceFlowHandle, DeviceFlowPoll, DeviceFlowStart, GatewayError, GitHubGateway,
    RepositoryPage, RepositorySort, RunStatus, RunStatusFilter, SecretToken, Workflow, WorkflowRun,
    WorkflowRunPage,
};

#[derive(Default)]
struct FakeGateway {
    workflows: Mutex<VecDeque<Result<Vec<Workflow>, GatewayError>>>,
    run_pages: Mutex<VecDeque<Result<WorkflowRunPage, GatewayError>>>,
    run_requests: Mutex<Vec<(Option<u64>, u32)>>,
}

impl FakeGateway {
    fn push_workflows(&self, response: Result<Vec<Workflow>, GatewayError>) {
        self.workflows.lock().unwrap().push_back(response);
    }

    fn push_runs(&self, response: Result<WorkflowRunPage, GatewayError>) {
        self.run_pages.lock().unwrap().push_back(response);
    }

    fn run_requests(&self) -> Vec<(Option<u64>, u32)> {
        self.run_requests.lock().unwrap().clone()
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
        self.workflows
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected(
                "no scripted workflows".to_owned(),
            )))
    }

    async fn list_workflow_runs(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        workflow_id: Option<u64>,
        page: u32,
        _per_page: u32,
    ) -> Result<WorkflowRunPage, GatewayError> {
        self.run_requests.lock().unwrap().push((workflow_id, page));
        self.run_pages
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected("no scripted runs".to_owned())))
    }

    async fn list_jobs(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _run_id: u64,
    ) -> Result<Vec<github_action_console::github::Job>, GatewayError> {
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

    async fn rate_limit(
        &self,
        _token: &SecretToken,
    ) -> Result<github_action_console::github::RateLimit, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }
}

fn workflow(id: u64, name: &str) -> Workflow {
    Workflow {
        id,
        name: name.to_owned(),
        path: format!(".github/workflows/{name}.yml"),
    }
}

fn run(
    id: u64,
    workflow_id: u64,
    status: RunStatus,
    conclusion: Option<&str>,
    branch: &str,
) -> WorkflowRun {
    WorkflowRun {
        id,
        workflow_id,
        name: format!("run {id}"),
        status,
        conclusion: conclusion.map(str::to_owned),
        branch: Some(branch.to_owned()),
        event: "push".to_owned(),
        actor: Some("octocat".to_owned()),
        created_at: Some("2026-09-22T10:00:00Z".to_owned()),
        html_url: Some(format!("https://github.com/octo/alpha/actions/runs/{id}")),
    }
}

fn page(runs: Vec<WorkflowRun>, has_more: bool) -> WorkflowRunPage {
    WorkflowRunPage { runs, has_more }
}

fn token() -> SecretToken {
    SecretToken::new("ghp_test_token")
}

fn entered(full_name: &str) -> Workspace {
    let mut workspace = Workspace::new();
    workspace.enter(full_name);
    workspace
}

#[tokio::test]
async fn loads_the_repositorys_workflows() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_workflows(Ok(vec![workflow(1, "ci"), workflow(2, "release")]));
    let mut workspace = entered("octo/alpha");

    workspace.load_workflows(&*gateway, &token()).await;

    assert_eq!(workspace.workflows_state(), LoadState::Loaded);
    assert_eq!(workspace.workflows().len(), 2);
    assert_eq!(workspace.workflows()[1].name, "release");
}

#[tokio::test]
async fn run_rows_carry_status_branch_event_actor_and_time() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_runs(Ok(page(
        vec![run(10, 1, RunStatus::Completed, Some("success"), "main")],
        false,
    )));
    let mut workspace = entered("octo/alpha");

    workspace.reload_runs(&*gateway, &token()).await;

    let run = &workspace.runs()[0];
    assert_eq!(run.status, RunStatus::Completed);
    assert_eq!(run.conclusion.as_deref(), Some("success"));
    assert_eq!(run.branch.as_deref(), Some("main"));
    assert_eq!(run.event, "push");
    assert_eq!(run.actor.as_deref(), Some("octocat"));
    assert_eq!(run.created_at.as_deref(), Some("2026-09-22T10:00:00Z"));
}

#[tokio::test]
async fn status_filter_separates_running_from_completed() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_runs(Ok(page(
        vec![
            run(1, 1, RunStatus::InProgress, None, "main"),
            run(2, 1, RunStatus::Completed, Some("success"), "main"),
        ],
        false,
    )));
    let mut workspace = entered("octo/alpha");
    workspace.reload_runs(&*gateway, &token()).await;

    workspace.set_status_filter(RunStatusFilter::Running);
    assert_eq!(workspace.visible_runs().len(), 1);
    assert_eq!(workspace.visible_runs()[0].id, 1);

    workspace.set_status_filter(RunStatusFilter::Completed);
    assert_eq!(workspace.visible_runs().len(), 1);
    assert_eq!(workspace.visible_runs()[0].id, 2);

    workspace.set_status_filter(RunStatusFilter::All);
    assert_eq!(workspace.visible_runs().len(), 2);
}

#[tokio::test]
async fn branch_filter_matches_exactly() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_runs(Ok(page(
        vec![
            run(1, 1, RunStatus::Completed, Some("success"), "main"),
            run(2, 1, RunStatus::Completed, Some("success"), "release/1.0"),
        ],
        false,
    )));
    let mut workspace = entered("octo/alpha");
    workspace.reload_runs(&*gateway, &token()).await;

    workspace.set_branch_filter(Some("release/1.0".to_owned()));

    assert_eq!(workspace.visible_runs().len(), 1);
    assert_eq!(workspace.visible_runs()[0].id, 2);
}

#[tokio::test]
async fn blank_branch_filter_is_ignored() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_runs(Ok(page(
        vec![run(1, 1, RunStatus::Completed, Some("success"), "main")],
        false,
    )));
    let mut workspace = entered("octo/alpha");
    workspace.reload_runs(&*gateway, &token()).await;

    workspace.set_branch_filter(Some("   ".to_owned()));

    assert_eq!(workspace.visible_runs().len(), 1);
}

#[tokio::test]
async fn workflow_filter_is_pushed_to_the_fetch() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_runs(Ok(page(
        vec![run(1, 7, RunStatus::Completed, Some("success"), "main")],
        false,
    )));
    let mut workspace = entered("octo/alpha");

    workspace.set_workflow_filter(Some(7));
    workspace.reload_runs(&*gateway, &token()).await;

    assert_eq!(gateway.run_requests(), vec![(Some(7), 1)]);
    assert_eq!(workspace.visible_runs().len(), 1);
}

#[tokio::test]
async fn a_running_run_makes_the_workspace_poll_only_in_the_runs_tab() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_runs(Ok(page(
        vec![run(1, 1, RunStatus::InProgress, None, "main")],
        false,
    )));
    let mut workspace = entered("octo/alpha");
    workspace.reload_runs(&*gateway, &token()).await;

    // Default tab is workflows: nothing to poll yet.
    assert!(!workspace.should_poll_runs());

    workspace.set_tab(WorkspaceTab::Runs);
    assert!(workspace.should_poll_runs());
}

#[tokio::test]
async fn polling_stops_once_every_run_has_completed() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_runs(Ok(page(
        vec![run(1, 1, RunStatus::InProgress, None, "main")],
        false,
    )));
    gateway.push_runs(Ok(page(
        vec![run(1, 1, RunStatus::Completed, Some("success"), "main")],
        false,
    )));
    let mut workspace = entered("octo/alpha");
    workspace.set_tab(WorkspaceTab::Runs);

    workspace.reload_runs(&*gateway, &token()).await;
    assert!(workspace.should_poll_runs());

    workspace.reload_runs(&*gateway, &token()).await;
    assert!(!workspace.should_poll_runs());
}

#[tokio::test]
async fn leaving_the_repository_stops_polling() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_runs(Ok(page(
        vec![run(1, 1, RunStatus::InProgress, None, "main")],
        false,
    )));
    let mut workspace = entered("octo/alpha");
    workspace.set_tab(WorkspaceTab::Runs);
    workspace.reload_runs(&*gateway, &token()).await;
    assert!(workspace.should_poll_runs());

    workspace.leave();

    assert!(!workspace.should_poll_runs());
    assert_eq!(workspace.repository(), None);
}

#[tokio::test]
async fn switching_repository_resets_filters_and_tab() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_runs(Ok(page(
        vec![run(1, 1, RunStatus::InProgress, None, "main")],
        false,
    )));
    let mut workspace = entered("octo/alpha");
    workspace.set_tab(WorkspaceTab::Runs);
    workspace.set_workflow_filter(Some(7));
    workspace.set_branch_filter(Some("main".to_owned()));
    workspace.reload_runs(&*gateway, &token()).await;

    workspace.enter("octo/beta");

    assert_eq!(workspace.repository(), Some("octo/beta"));
    assert_eq!(workspace.tab(), WorkspaceTab::Workflows);
    assert_eq!(workspace.run_filter().workflow_id, None);
    assert_eq!(workspace.run_filter().branch, None);
    assert!(workspace.runs().is_empty());
}

#[tokio::test]
async fn runs_paginate() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_runs(Ok(page(
        vec![run(1, 1, RunStatus::Completed, Some("success"), "main")],
        true,
    )));
    gateway.push_runs(Ok(page(
        vec![run(2, 1, RunStatus::Completed, Some("success"), "main")],
        false,
    )));
    let mut workspace = entered("octo/alpha");

    workspace.reload_runs(&*gateway, &token()).await;
    workspace.load_more_runs(&*gateway, &token()).await;

    assert_eq!(workspace.runs().len(), 2);
    assert!(!workspace.runs_has_more());
    assert_eq!(gateway.run_requests(), vec![(None, 1), (None, 2)]);
}

#[tokio::test]
async fn transport_failure_is_surfaced_on_the_runs_tab() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_runs(Err(GatewayError::Transport("offline".to_owned())));
    let mut workspace = entered("octo/alpha");

    workspace.reload_runs(&*gateway, &token()).await;

    assert_eq!(
        workspace.runs_state(),
        LoadState::Failed(github_action_console::app::AppProblem::Network)
    );
}
