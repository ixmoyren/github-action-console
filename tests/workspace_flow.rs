use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use github_action_console::app::{
    AppProblem, LoadState, RunProblem, SaveProblem, Workspace, WorkspaceTab,
};
use github_action_console::github::{
    Account, DeviceFlowHandle, DeviceFlowPoll, DeviceFlowStart, FileContents, FileWrite,
    GatewayError, GitHubGateway, RepositoryPage, RepositorySort, RunStatus, RunStatusFilter,
    SecretToken, Workflow, WorkflowRun, WorkflowRunPage,
};

#[derive(Default)]
struct FakeGateway {
    workflows: Mutex<VecDeque<Result<Vec<Workflow>, GatewayError>>>,
    files: Mutex<VecDeque<Result<FileContents, GatewayError>>>,
    file_requests: Mutex<Vec<String>>,
    writes: Mutex<Vec<FileWrite>>,
    write_results: Mutex<VecDeque<Result<(), GatewayError>>>,
    dispatches: Mutex<Vec<(u64, String)>>,
    dispatch_results: Mutex<VecDeque<Result<(), GatewayError>>>,
    run_pages: Mutex<VecDeque<Result<WorkflowRunPage, GatewayError>>>,
    run_requests: Mutex<Vec<(Option<u64>, u32)>>,
}

impl FakeGateway {
    fn push_workflows(&self, response: Result<Vec<Workflow>, GatewayError>) {
        self.workflows.lock().unwrap().push_back(response);
    }

    fn push_file(&self, response: Result<FileContents, GatewayError>) {
        self.files.lock().unwrap().push_back(response);
    }

    fn push_file_text(&self, text: &str, sha: &str) {
        self.push_file(Ok(FileContents {
            text: text.to_owned(),
            sha: sha.to_owned(),
        }));
    }

    fn file_requests(&self) -> Vec<String> {
        self.file_requests.lock().unwrap().clone()
    }

    fn push_write(&self, response: Result<(), GatewayError>) {
        self.write_results.lock().unwrap().push_back(response);
    }

    fn writes(&self) -> Vec<FileWrite> {
        self.writes.lock().unwrap().clone()
    }

    fn push_dispatch(&self, response: Result<(), GatewayError>) {
        self.dispatch_results.lock().unwrap().push_back(response);
    }

    fn dispatches(&self) -> Vec<(u64, String)> {
        self.dispatches.lock().unwrap().clone()
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

    async fn file_contents(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        path: &str,
    ) -> Result<FileContents, GatewayError> {
        self.file_requests.lock().unwrap().push(path.to_owned());
        self.files
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected(
                "no scripted workflow file".to_owned(),
            )))
    }

    async fn update_file(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        write: FileWrite,
    ) -> Result<(), GatewayError> {
        self.writes.lock().unwrap().push(write);
        self.write_results
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected(
                "no scripted write".to_owned(),
            )))
    }

    async fn dispatch_workflow(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        workflow_id: u64,
        reference: &str,
    ) -> Result<(), GatewayError> {
        self.dispatches
            .lock()
            .unwrap()
            .push((workflow_id, reference.to_owned()));
        self.dispatch_results
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected(
                "no scripted dispatch".to_owned(),
            )))
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

    fn set_proxy(&self, _proxy: Option<String>) {}

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
    workspace.enter(full_name, Some("main".to_owned()));
    workspace
}

#[tokio::test]
async fn loads_the_repositorys_workflows() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_workflows(Ok(vec![workflow(1, "ci"), workflow(2, "release")]));
    gateway.push_file_text("name: ci\n", "sha-ci");
    let mut workspace = entered("octo/alpha");

    workspace.load_workflows(&*gateway, &token()).await;

    assert_eq!(workspace.workflows_state(), LoadState::Loaded);
    assert_eq!(workspace.workflows().len(), 2);
    assert_eq!(workspace.workflows()[1].name, "release");
}

#[tokio::test]
async fn opening_a_repository_defaults_to_the_first_workflow() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_workflows(Ok(vec![workflow(1, "ci"), workflow(2, "release")]));
    gateway.push_file_text("name: ci\n", "sha-ci");
    let mut workspace = entered("octo/alpha");

    workspace.load_workflows(&*gateway, &token()).await;

    assert_eq!(workspace.selected_workflow_id(), Some(1));
    assert_eq!(workspace.selected_workflow().unwrap().name, "ci");
    assert_eq!(workspace.workflow_file_state(), LoadState::Loaded);
    assert_eq!(workspace.workflow_file(), Some("name: ci\n"));
    assert_eq!(
        gateway.file_requests(),
        vec![".github/workflows/ci.yml".to_owned()]
    );
}

#[tokio::test]
async fn a_repository_without_workflows_selects_nothing() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_workflows(Ok(Vec::new()));
    let mut workspace = entered("octo/alpha");

    workspace.load_workflows(&*gateway, &token()).await;

    assert_eq!(workspace.selected_workflow_id(), None);
    assert!(gateway.file_requests().is_empty());
}

#[tokio::test]
async fn selecting_another_workflow_reads_its_file() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_workflows(Ok(vec![workflow(1, "ci"), workflow(2, "release")]));
    gateway.push_file_text("name: ci\n", "sha-ci");
    let mut workspace = entered("octo/alpha");
    workspace.load_workflows(&*gateway, &token()).await;

    gateway.push_file_text("name: release\n", "sha-release");
    workspace.select_workflow(2);
    workspace.load_workflow_file(&*gateway, &token()).await;

    assert_eq!(workspace.selected_workflow_id(), Some(2));
    assert_eq!(workspace.workflow_file(), Some("name: release\n"));
    assert_eq!(
        gateway.file_requests(),
        vec![
            ".github/workflows/ci.yml".to_owned(),
            ".github/workflows/release.yml".to_owned()
        ]
    );
}

#[tokio::test]
async fn a_missing_workflow_file_is_surfaced() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_workflows(Ok(vec![workflow(1, "ci")]));
    gateway.push_file(Err(GatewayError::NotFound));
    let mut workspace = entered("octo/alpha");

    workspace.load_workflows(&*gateway, &token()).await;

    assert_eq!(
        workspace.workflow_file_state(),
        LoadState::Failed(AppProblem::NotFound)
    );
    assert_eq!(workspace.workflow_file(), None);
}

#[tokio::test]
async fn running_the_selected_workflow_dispatches_it_on_the_default_branch() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_workflows(Ok(vec![workflow(1, "ci"), workflow(2, "release")]));
    gateway.push_file_text("name: ci\n", "sha-ci");
    gateway.push_dispatch(Ok(()));
    let mut workspace = entered("octo/alpha");
    workspace.load_workflows(&*gateway, &token()).await;

    let outcome = workspace.run_selected_workflow(&*gateway, &token()).await;

    assert_eq!(outcome, Ok(()));
    assert_eq!(gateway.dispatches(), vec![(1, "main".to_owned())]);
}

#[tokio::test]
async fn saving_a_workflow_file_commits_it_on_the_default_branch() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_workflows(Ok(vec![workflow(1, "ci")]));
    gateway.push_file_text("name: ci\n", "sha-ci");
    gateway.push_write(Ok(()));
    // The console reads the file back after a successful write.
    gateway.push_file_text("name: ci\non: push\n", "sha-next");
    let mut workspace = entered("octo/alpha");
    workspace.load_workflows(&*gateway, &token()).await;

    let outcome = workspace
        .save_workflow_file(&*gateway, &token(), "name: ci\non: push\n")
        .await;

    assert_eq!(outcome, Ok(()));
    let writes = gateway.writes();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].path, ".github/workflows/ci.yml");
    assert_eq!(writes[0].contents, "name: ci\non: push\n");
    assert_eq!(writes[0].reference, "main");
    assert_eq!(writes[0].sha, "sha-ci");
    assert!(writes[0].message.contains(".github/workflows/ci.yml"));
    // The reload means the console now holds what the repository holds.
    assert_eq!(workspace.workflow_file(), Some("name: ci\non: push\n"));
}

#[tokio::test]
async fn saving_without_a_read_revision_is_refused() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_workflows(Ok(Vec::new()));
    let mut workspace = entered("octo/alpha");
    workspace.load_workflows(&*gateway, &token()).await;

    let outcome = workspace
        .save_workflow_file(&*gateway, &token(), "name: ci\n")
        .await;

    assert_eq!(outcome, Err(SaveProblem::NoWorkflow));
    assert!(gateway.writes().is_empty());
}

#[tokio::test]
async fn saving_without_a_default_branch_is_refused() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_workflows(Ok(vec![workflow(1, "ci")]));
    gateway.push_file_text("name: ci\n", "sha-ci");
    let mut workspace = Workspace::new();
    workspace.enter("octo/alpha", None);
    workspace.load_workflows(&*gateway, &token()).await;

    let outcome = workspace
        .save_workflow_file(&*gateway, &token(), "name: ci\n")
        .await;

    assert_eq!(outcome, Err(SaveProblem::NoDefaultBranch));
    assert!(gateway.writes().is_empty());
}

#[tokio::test]
async fn a_refused_write_keeps_its_gateway_problem() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_workflows(Ok(vec![workflow(1, "ci")]));
    gateway.push_file_text("name: ci\n", "sha-ci");
    gateway.push_write(Err(GatewayError::Forbidden));
    let mut workspace = entered("octo/alpha");
    workspace.load_workflows(&*gateway, &token()).await;

    let outcome = workspace
        .save_workflow_file(&*gateway, &token(), "name: ci\non: push\n")
        .await;

    assert_eq!(outcome, Err(SaveProblem::Gateway(AppProblem::Forbidden)));
}

#[tokio::test]
async fn running_without_a_default_branch_is_refused() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_workflows(Ok(vec![workflow(1, "ci")]));
    gateway.push_file_text("name: ci\n", "sha-ci");
    let mut workspace = Workspace::new();
    workspace.enter("octo/alpha", None);
    workspace.load_workflows(&*gateway, &token()).await;

    let outcome = workspace.run_selected_workflow(&*gateway, &token()).await;

    assert_eq!(outcome, Err(RunProblem::NoDefaultBranch));
    assert!(gateway.dispatches().is_empty());
}

#[tokio::test]
async fn running_without_a_selection_is_refused() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_workflows(Ok(Vec::new()));
    let mut workspace = entered("octo/alpha");
    workspace.load_workflows(&*gateway, &token()).await;

    let outcome = workspace.run_selected_workflow(&*gateway, &token()).await;

    assert_eq!(outcome, Err(RunProblem::NoWorkflow));
    assert!(gateway.dispatches().is_empty());
}

#[tokio::test]
async fn a_refused_dispatch_keeps_its_gateway_problem() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_workflows(Ok(vec![workflow(1, "ci")]));
    gateway.push_file_text("name: ci\n", "sha-ci");
    gateway.push_dispatch(Err(GatewayError::Forbidden));
    let mut workspace = entered("octo/alpha");
    workspace.load_workflows(&*gateway, &token()).await;

    let outcome = workspace.run_selected_workflow(&*gateway, &token()).await;

    assert_eq!(outcome, Err(RunProblem::Gateway(AppProblem::Forbidden)));
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

    workspace.enter("octo/beta", Some("develop".to_owned()));

    assert_eq!(workspace.repository(), Some("octo/beta"));
    assert_eq!(workspace.default_branch(), Some("develop"));
    assert_eq!(workspace.tab(), WorkspaceTab::Workflows);
    assert_eq!(workspace.run_filter().workflow_id, None);
    assert_eq!(workspace.run_filter().branch, None);
    assert_eq!(workspace.selected_workflow_id(), None);
    assert_eq!(workspace.workflow_file(), None);
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
