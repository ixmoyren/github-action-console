use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use github_action_console::app::{
    AppProblem, CreateProblem, LoadState, PushOutcome, RunActionProblem, RunProblem, SaveProblem,
    Workspace, WorkspaceTab,
};
use github_action_console::github::{
    Account, DeviceFlowHandle, DeviceFlowPoll, DeviceFlowStart, FileContents, FileWrite,
    GatewayError, GitHubGateway, PullRequest, ReleaseAsset, RepositoryPage, RepositorySort,
    RunStatus, RunStatusFilter, SecretToken, Workflow, WorkflowRun, WorkflowRunPage,
};
use github_action_console::release_template;
use github_action_console::workflow_draft::{DraftProblem, JobDraft, Triggers, WorkflowDraft};

/// One workflow dispatch the console asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Dispatch {
    workflow: String,
    reference: String,
    inputs: Vec<(String, String)>,
}

#[derive(Default)]
struct FakeGateway {
    workflows: Mutex<VecDeque<Result<Vec<Workflow>, GatewayError>>>,
    files: Mutex<VecDeque<Result<FileContents, GatewayError>>>,
    file_requests: Mutex<Vec<String>>,
    runner_labels: Mutex<VecDeque<Result<Vec<String>, GatewayError>>>,
    writes: Mutex<Vec<FileWrite>>,
    write_results: Mutex<VecDeque<Result<(), GatewayError>>>,
    dispatches: Mutex<Vec<Dispatch>>,
    dispatch_results: Mutex<VecDeque<Result<(), GatewayError>>>,
    run_pages: Mutex<VecDeque<Result<WorkflowRunPage, GatewayError>>>,
    run_requests: Mutex<Vec<(Option<u64>, u32)>>,
    cancelled: Mutex<Vec<u64>>,
    deleted: Mutex<Vec<u64>>,
    run_action_results: Mutex<VecDeque<Result<(), GatewayError>>>,
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

    fn push_runner_labels(&self, response: Result<Vec<String>, GatewayError>) {
        self.runner_labels.lock().unwrap().push_back(response);
    }

    fn writes(&self) -> Vec<FileWrite> {
        self.writes.lock().unwrap().clone()
    }

    fn push_dispatch(&self, response: Result<(), GatewayError>) {
        self.dispatch_results.lock().unwrap().push_back(response);
    }

    fn dispatches(&self) -> Vec<(u64, String)> {
        self.dispatches
            .lock()
            .unwrap()
            .iter()
            .map(|dispatch| {
                (
                    dispatch.workflow.parse().unwrap_or_default(),
                    dispatch.reference.clone(),
                )
            })
            .collect()
    }

    fn push_runs(&self, response: Result<WorkflowRunPage, GatewayError>) {
        self.run_pages.lock().unwrap().push_back(response);
    }

    fn run_requests(&self) -> Vec<(Option<u64>, u32)> {
        self.run_requests.lock().unwrap().clone()
    }

    fn push_run_action(&self, response: Result<(), GatewayError>) {
        self.run_action_results.lock().unwrap().push_back(response);
    }

    fn cancelled(&self) -> Vec<u64> {
        self.cancelled.lock().unwrap().clone()
    }

    fn deleted(&self) -> Vec<u64> {
        self.deleted.lock().unwrap().clone()
    }

    fn next_run_action(&self) -> Result<(), GatewayError> {
        self.run_action_results
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected(
                "no scripted run action".to_owned(),
            )))
    }
}

#[async_trait]
impl GitHubGateway for FakeGateway {
    async fn branches(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
    ) -> Result<Vec<String>, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn branch_commits(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _branch: &str,
        _limit: u8,
    ) -> Result<Vec<github_action_console::github::CommitSummary>, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn create_tag(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _tag: &str,
        _sha: &str,
    ) -> Result<(), GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

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

    async fn runner_labels(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
    ) -> Result<Vec<String>, GatewayError> {
        self.runner_labels
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected(
                "no scripted runner labels".to_owned(),
            )))
    }

    async fn write_file(
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
        workflow: &str,
        reference: &str,
        inputs: &[(String, String)],
    ) -> Result<(), GatewayError> {
        self.dispatches.lock().unwrap().push(Dispatch {
            workflow: workflow.to_owned(),
            reference: reference.to_owned(),
            inputs: inputs.to_vec(),
        });
        self.dispatch_results
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected(
                "no scripted dispatch".to_owned(),
            )))
    }

    async fn cancel_workflow_run(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        run_id: u64,
    ) -> Result<(), GatewayError> {
        self.cancelled.lock().unwrap().push(run_id);
        self.next_run_action()
    }

    async fn delete_workflow_run(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        run_id: u64,
    ) -> Result<(), GatewayError> {
        self.deleted.lock().unwrap().push(run_id);
        self.next_run_action()
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

/// The form as the console fills it in for a new "nightly" workflow.
fn draft(file_name: &str) -> WorkflowDraft {
    WorkflowDraft {
        file_name: file_name.to_owned(),
        name: "Nightly".to_owned(),
        runs_on: "ubuntu-latest".to_owned(),
        container: Some("node:20-bullseye".to_owned()),
        triggers: Triggers::default(),
        jobs: vec![JobDraft {
            id: "build".to_owned(),
            name: "Build".to_owned(),
            command: "npm test".to_owned(),
        }],
    }
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
async fn the_console_learns_which_workflows_can_be_run_by_hand() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_workflows(Ok(vec![workflow(1, "release-target"), workflow(2, "ci")]));
    // 选中第一条时编辑器读的那一份，接着是扫描两条各自的文件。
    gateway.push_file_text("name: Release target\non:\n  workflow_dispatch:\n", "sha-1");
    gateway.push_file_text("name: Release target\non:\n  workflow_dispatch:\n", "sha-1");
    gateway.push_file_text("name: ci\non: push\n", "sha-2");
    let mut workspace = entered("octo/alpha");

    workspace.load_workflows(&*gateway, &token()).await;
    workspace.load_workflow_triggers(&*gateway, &token()).await;

    // 有 workflow_dispatch 的能跑，只有 push 的不能。
    assert!(workspace.can_run(1));
    assert!(!workspace.can_run(2));

    // 已经读过的不再读：再来一遍不会再发请求。
    let reads = gateway.file_requests().len();
    workspace.load_workflow_triggers(&*gateway, &token()).await;
    assert_eq!(gateway.file_requests().len(), reads);
}

#[tokio::test]
async fn a_workflow_file_that_could_not_be_read_gets_another_chance() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_workflows(Ok(vec![workflow(1, "ci")]));
    // 编辑器读到的失败是一次性的；扫描时再读一次，这次读到了内容。
    gateway.push_file(Err(GatewayError::Transport("offline".to_owned())));
    gateway.push_file(Err(GatewayError::Transport("offline".to_owned())));
    gateway.push_file_text("name: ci\non:\n  workflow_dispatch:\n", "sha-1");
    let mut workspace = entered("octo/alpha");

    workspace.load_workflows(&*gateway, &token()).await;
    workspace.load_workflow_triggers(&*gateway, &token()).await;

    // 第一次没读到：不下结论。
    assert!(!workspace.can_run(1));

    workspace.load_workflow_triggers(&*gateway, &token()).await;

    // 再读一次读到了：能跑。
    assert!(workspace.can_run(1));
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
    assert_eq!(writes[0].sha.as_deref(), Some("sha-ci"));
    assert!(writes[0].message.contains(".github/workflows/ci.yml"));
    // The reload means the console now holds what the repository holds.
    assert_eq!(workspace.workflow_file(), Some("name: ci\non: push\n"));
}

#[tokio::test]
async fn creating_a_workflow_pushes_the_generated_file_and_shows_it() {
    let gateway = Arc::new(FakeGateway::default());
    // The name is free, the write succeeds, and the list then holds it.
    gateway.push_file(Err(GatewayError::NotFound));
    gateway.push_write(Ok(()));
    gateway.push_workflows(Ok(vec![workflow(9, "nightly")]));
    gateway.push_file_text("name: Nightly\n", "sha-new");
    let mut workspace = entered("octo/alpha");

    let outcome = workspace
        .create_workflow(&*gateway, &token(), &draft("nightly"))
        .await;

    assert_eq!(outcome, Ok(PushOutcome::Created));
    let writes = gateway.writes();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].path, ".github/workflows/nightly.yml");
    // A new file has no revision to start from.
    assert_eq!(writes[0].sha, None);
    assert_eq!(writes[0].reference, "main");
    assert!(writes[0].contents.contains("name: Nightly\n"));
    assert!(writes[0].contents.contains("runs-on: ubuntu-latest\n"));
    assert!(
        writes[0]
            .contents
            .contains("container: \"node:20-bullseye\"\n")
    );
    assert!(writes[0].contents.contains("- run: npm test\n"));
    assert!(writes[0].message.contains(".github/workflows/nightly.yml"));
    // The new workflow is selected, and its file is what the editor will show.
    assert_eq!(workspace.selected_workflow_id(), Some(9));
    assert_eq!(workspace.workflow_file(), Some("name: Nightly\n"));
}

#[tokio::test]
async fn pushing_a_workflow_that_already_exists_replaces_it() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_file_text("name: Nightly\n", "sha-there");
    gateway.push_write(Ok(()));
    gateway.push_workflows(Ok(vec![workflow(9, "nightly")]));
    gateway.push_file_text("name: Nightly\n", "sha-next");
    let mut workspace = entered("octo/alpha");

    let outcome = workspace
        .create_workflow(&*gateway, &token(), &draft("nightly"))
        .await;

    assert_eq!(outcome, Ok(PushOutcome::Replaced));
    let writes = gateway.writes();
    assert_eq!(writes.len(), 1);
    // The replacement names the revision it starts from, so nothing is lost.
    assert_eq!(writes[0].sha.as_deref(), Some("sha-there"));
    assert!(writes[0].message.contains("更新"));
}

/// 采用发布模板：文本不由表单生成，直接写进仓库的那个固定路径。
#[tokio::test]
async fn adopting_the_release_template_writes_it_to_the_workflows_directory() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_file(Err(GatewayError::NotFound));
    gateway.push_write(Ok(()));
    gateway.push_workflows(Ok(vec![]));
    let mut workspace = entered("octo/alpha");
    let template = github_action_console::release_template::for_repository("octo/alpha");

    let outcome = workspace
        .push_workflow_file(
            &*gateway,
            &token(),
            github_action_console::release_template::TEMPLATE_PATH,
            &template,
            "发布模板",
        )
        .await;

    assert_eq!(
        outcome.as_ref().map(|(outcome, _)| *outcome),
        Ok(PushOutcome::Created)
    );
    let writes = gateway.writes();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].path, ".github/workflows/release-target.yml");
    // 新文件没有起始版本，默认分支是写入的参考点。
    assert_eq!(writes[0].sha, None);
    assert_eq!(writes[0].reference, "main");
    // 写的是模板正文本身，并且带着"这是给谁的"那一行。
    assert!(writes[0].contents.contains("octo/alpha"));
    assert!(writes[0].contents.contains("gh release upload"));
    assert!(writes[0].message.contains("发布模板"));
}

/// 采用模板之后，仓库里真的多了一条工作流：名字用模板里的那一行，并且它就在屏幕上。
#[tokio::test]
async fn adopting_the_release_template_opens_the_workflow_it_just_wrote() {
    let gateway = Arc::new(FakeGateway::default());
    // 先查这个名字是不是空的，再写，写完列表里就有它了，最后把正文读回来。
    gateway.push_file(Err(GatewayError::NotFound));
    gateway.push_write(Ok(()));
    gateway.push_workflows(Ok(vec![workflow(11, "release-target")]));
    gateway.push_file_text("name: Release target\n", "sha-1");
    let mut workspace = entered("octo/alpha");
    let contents = release_template::for_repository("octo/alpha");

    let outcome = workspace
        .push_workflow_file(
            &*gateway,
            &token(),
            release_template::TEMPLATE_PATH,
            &contents,
            "发布模板",
        )
        .await;

    assert!(outcome.is_ok(), "{outcome:?}");
    // 工作流的名字来自模板：`name:` 那一行原样写进去，不另起名字。
    let name_line = release_template::TEMPLATE
        .lines()
        .find(|line| line.starts_with("name:"))
        .expect("模板里有 name:");
    let writes = gateway.writes();
    assert_eq!(writes.len(), 1);
    assert!(
        writes[0].contents.contains(name_line),
        "写进去的不是模板里那个名字：{name_line:?}"
    );
    // 新建的这一条就是屏幕上的那一条。
    assert_eq!(workspace.selected_workflow_id(), Some(11));
    assert_eq!(workspace.workflow_file(), Some("name: Release target\n"));
}

#[tokio::test]
async fn adopting_the_release_template_replaces_an_older_copy() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_file_text("name: Release target\n", "sha-there");
    gateway.push_write(Ok(()));
    gateway.push_workflows(Ok(vec![]));
    let mut workspace = entered("octo/alpha");

    let outcome = workspace
        .push_workflow_file(
            &*gateway,
            &token(),
            github_action_console::release_template::TEMPLATE_PATH,
            "name: Release target\n",
            "发布模板",
        )
        .await;

    assert_eq!(
        outcome.as_ref().map(|(outcome, _)| *outcome),
        Ok(PushOutcome::Replaced)
    );
    let writes = gateway.writes();
    // 覆盖要带上原来那一版的 revision。
    assert_eq!(writes[0].sha.as_deref(), Some("sha-there"));
}

#[tokio::test]
async fn a_draft_that_is_not_a_workflow_never_reaches_github() {
    let gateway = Arc::new(FakeGateway::default());
    let mut workspace = entered("octo/alpha");
    let mut incomplete = draft("nightly");
    incomplete.jobs.clear();

    let outcome = workspace
        .create_workflow(&*gateway, &token(), &incomplete)
        .await;
    assert_eq!(outcome, Err(CreateProblem::Draft(DraftProblem::NoJobs)));

    let mut bad_name = draft("release/notes");
    let outcome = workspace
        .create_workflow(&*gateway, &token(), &bad_name)
        .await;
    assert_eq!(outcome, Err(CreateProblem::Draft(DraftProblem::FileName)));
    bad_name.file_name = "nightly".to_owned();

    bad_name.jobs[0].id = "1build".to_owned();
    let outcome = workspace
        .create_workflow(&*gateway, &token(), &bad_name)
        .await;
    assert_eq!(outcome, Err(CreateProblem::Draft(DraftProblem::JobId)));

    assert!(gateway.file_requests().is_empty());
    assert!(gateway.writes().is_empty());
}

#[tokio::test]
async fn a_failed_name_check_stops_the_workflow_before_it_is_written() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_file(Err(GatewayError::Transport("offline".to_owned())));
    let mut workspace = entered("octo/alpha");

    let outcome = workspace
        .create_workflow(&*gateway, &token(), &draft("nightly"))
        .await;

    assert_eq!(outcome, Err(CreateProblem::Gateway(AppProblem::Network)));
    assert!(gateway.writes().is_empty());
}

#[tokio::test]
async fn runner_labels_come_back_sorted_and_without_duplicates() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_runner_labels(Ok(vec![
        "linux".to_owned(),
        "x64".to_owned(),
        "linux".to_owned(),
        "  gpu  ".to_owned(),
        "   ".to_owned(),
    ]));
    let mut workspace = entered("octo/alpha");

    workspace.load_runner_labels(&*gateway, &token()).await;

    assert_eq!(workspace.runner_labels_state(), LoadState::Loaded);
    assert_eq!(workspace.runner_labels(), ["gpu", "linux", "x64"]);
}

#[tokio::test]
async fn a_repository_that_will_not_share_its_runners_keeps_the_hosted_ones() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_runner_labels(Err(GatewayError::Forbidden));
    let mut workspace = entered("octo/alpha");

    workspace.load_runner_labels(&*gateway, &token()).await;

    assert_eq!(
        workspace.runner_labels_state(),
        LoadState::Failed(AppProblem::Forbidden)
    );
    assert!(workspace.runner_labels().is_empty());
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

/// 取消一次运行：问 GitHub 取消，然后把列表重新读一遍（状态要跟着变）。
#[tokio::test]
async fn cancelling_a_run_tells_github_and_reloads_the_list() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_run_action(Ok(()));
    gateway.push_runs(Ok(page(
        vec![run(1, 7, RunStatus::InProgress, None, "main")],
        false,
    )));
    gateway.push_runs(Ok(page(
        vec![run(1, 7, RunStatus::Completed, Some("cancelled"), "main")],
        false,
    )));
    let mut workspace = entered("octo/alpha");
    workspace.reload_runs(&*gateway, &token()).await;

    let outcome = workspace.cancel_run(&*gateway, &token(), 1).await;

    assert_eq!(outcome, Ok(()));
    assert_eq!(gateway.cancelled(), vec![1]);
    assert!(gateway.deleted().is_empty());
    // 取消之后又读了一遍：列表里那一条已经是被取消的结论。
    assert_eq!(gateway.run_requests(), vec![(None, 1), (None, 1)]);
    assert_eq!(
        workspace.visible_runs()[0].conclusion.as_deref(),
        Some("cancelled")
    );
}

/// 删除一次运行：问 GitHub 删除，然后重新读列表。
#[tokio::test]
async fn deleting_a_run_tells_github_and_reloads_the_list() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_run_action(Ok(()));
    gateway.push_runs(Ok(page(
        vec![run(1, 7, RunStatus::Completed, Some("success"), "main")],
        false,
    )));
    gateway.push_runs(Ok(page(Vec::new(), false)));
    let mut workspace = entered("octo/alpha");
    workspace.reload_runs(&*gateway, &token()).await;

    let outcome = workspace.delete_run(&*gateway, &token(), 1).await;

    assert_eq!(outcome, Ok(()));
    assert_eq!(gateway.deleted(), vec![1]);
    assert!(gateway.cancelled().is_empty());
    // 删掉之后列表里就没有它了。
    assert!(workspace.visible_runs().is_empty());
}

/// GitHub 拒绝的时候，控制台照实说，不装作删掉了。
#[tokio::test]
async fn a_refused_run_action_is_surfaced() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_run_action(Err(GatewayError::Forbidden));
    let mut workspace = entered("octo/alpha");

    let outcome = workspace.delete_run(&*gateway, &token(), 1).await;

    assert_eq!(
        outcome,
        Err(RunActionProblem::Gateway(AppProblem::Forbidden))
    );
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
