//! The release board: manifest in, pointers out, and nothing else moving a
//! pointer — not even a green build.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use github_action_console::app::{
    AppProblem, ManifestState, PublishProblem, ReleaseBoard, TagProblem, TriggerProblem,
    version_covers_target,
};
use github_action_console::github::{
    Account, BuildArtifact, CommitSummary, DeviceFlowHandle, DeviceFlowPoll, DeviceFlowStart,
    FileContents, FileWrite, GatewayError, GitHubGateway, Job, PullRequest, RateLimit,
    ReleaseAsset, RepositoryPage, RepositorySort, RunStatus, SecretToken, Workflow, WorkflowRun,
    WorkflowRunPage,
};
use github_action_console::release::{CHANNELS, ReleaseState};
use github_action_console::store::Store;

/// One workflow dispatch the console asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Dispatch {
    workflow: String,
    reference: String,
    inputs: Vec<(String, String)>,
}

#[derive(Default)]
struct FakeGateway {
    files: Mutex<VecDeque<Result<FileContents, GatewayError>>>,
    assets: Mutex<VecDeque<Result<Vec<ReleaseAsset>, GatewayError>>>,
    releases: Mutex<Vec<String>>,
    dispatches: Mutex<Vec<Dispatch>>,
    dispatch_results: Mutex<VecDeque<Result<(), GatewayError>>>,
    writes: Mutex<Vec<FileWrite>>,
    branches: Mutex<Vec<(String, String)>>,
    pulls: Mutex<Vec<(String, String, String)>>,
    commits: Mutex<VecDeque<Result<Vec<CommitSummary>, GatewayError>>>,
    commit_reads: Mutex<Vec<(String, u8)>>,
    tags: Mutex<Vec<(String, String)>>,
    tag_results: Mutex<VecDeque<Result<(), GatewayError>>>,
    jobs: Mutex<VecDeque<Result<Vec<Job>, GatewayError>>>,
    job_reads: Mutex<Vec<u64>>,
}

impl FakeGateway {
    fn push_manifest(&self, text: &str) {
        self.files.lock().unwrap().push_back(Ok(FileContents {
            text: text.to_owned(),
            sha: "sha-manifest".to_owned(),
        }));
    }

    fn push_missing_manifest(&self) {
        self.files
            .lock()
            .unwrap()
            .push_back(Err(GatewayError::NotFound));
    }

    fn push_assets(&self, assets: Vec<ReleaseAsset>) {
        self.assets.lock().unwrap().push_back(Ok(assets));
    }

    fn push_missing_release(&self) {
        self.assets
            .lock()
            .unwrap()
            .push_back(Err(GatewayError::NotFound));
    }

    fn push_dispatch(&self, result: Result<(), GatewayError>) {
        self.dispatch_results.lock().unwrap().push_back(result);
    }

    fn dispatches(&self) -> Vec<Dispatch> {
        self.dispatches.lock().unwrap().clone()
    }

    fn release_lookups(&self) -> Vec<String> {
        self.releases.lock().unwrap().clone()
    }

    fn writes(&self) -> Vec<FileWrite> {
        self.writes.lock().unwrap().clone()
    }

    fn branches(&self) -> Vec<(String, String)> {
        self.branches.lock().unwrap().clone()
    }

    fn pulls(&self) -> Vec<(String, String, String)> {
        self.pulls.lock().unwrap().clone()
    }

    fn push_commits(&self, commits: Vec<CommitSummary>) {
        self.commits.lock().unwrap().push_back(Ok(commits));
    }

    fn commit_reads(&self) -> Vec<(String, u8)> {
        self.commit_reads.lock().unwrap().clone()
    }

    fn push_tag(&self, result: Result<(), GatewayError>) {
        self.tag_results.lock().unwrap().push_back(result);
    }

    /// 打过（或移过）的 tag：(tag, sha)。
    fn tags(&self) -> Vec<(String, String)> {
        self.tags.lock().unwrap().clone()
    }

    fn push_jobs(&self, jobs: Vec<Job>) {
        self.jobs.lock().unwrap().push_back(Ok(jobs));
    }

    fn job_reads(&self) -> Vec<u64> {
        self.job_reads.lock().unwrap().clone()
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
        branch: &str,
        limit: u8,
    ) -> Result<Vec<github_action_console::github::CommitSummary>, GatewayError> {
        self.commit_reads
            .lock()
            .unwrap()
            .push((branch.to_owned(), limit));
        self.commits
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected(
                "no scripted commits".to_owned(),
            )))
    }

    async fn create_tag(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        tag: &str,
        sha: &str,
    ) -> Result<(), GatewayError> {
        self.tags
            .lock()
            .unwrap()
            .push((tag.to_owned(), sha.to_owned()));
        self.tag_results
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(()))
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
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn file_contents(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _path: &str,
    ) -> Result<FileContents, GatewayError> {
        self.files
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected(
                "no scripted manifest".to_owned(),
            )))
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
        write: FileWrite,
    ) -> Result<(), GatewayError> {
        self.writes.lock().unwrap().push(write);
        Ok(())
    }

    async fn create_branch(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        branch: &str,
        from: &str,
    ) -> Result<(), GatewayError> {
        self.branches
            .lock()
            .unwrap()
            .push((branch.to_owned(), from.to_owned()));
        Ok(())
    }

    async fn open_pull_request(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        pull: PullRequest,
    ) -> Result<u64, GatewayError> {
        self.pulls
            .lock()
            .unwrap()
            .push((pull.head, pull.base, pull.title));
        Ok(42)
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
            .unwrap_or(Ok(()))
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

    async fn cancel_workflow_run(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _run_id: u64,
    ) -> Result<(), GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn delete_workflow_run(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _run_id: u64,
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
        self.job_reads.lock().unwrap().push(run_id);
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

    async fn release_assets(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        tag: &str,
    ) -> Result<Vec<ReleaseAsset>, GatewayError> {
        self.releases.lock().unwrap().push(tag.to_owned());
        self.assets
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected(
                "no scripted release".to_owned(),
            )))
    }

    async fn download_release_asset(
        &self,
        _token: &SecretToken,
        _url: &str,
    ) -> Result<Vec<u8>, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    fn set_proxy(&self, _proxy: Option<String>) {}

    async fn rate_limit(&self, _token: &SecretToken) -> Result<RateLimit, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }
}

/// The manifest the demo repository carries (`.github/release-console.yml`).
const MANIFEST: &str = "\
version: 1

targets:
  web-arm:
    platform: macos
    arch: arm64
    distribution: github-releases
    packaging: macos-dmg
  web-intel:
    platform: macos
    arch: x64
    distribution: github-releases
    packaging: macos-dmg
  windows:
    platform: windows
    arch: x64
    distribution: github-releases
    packaging: windows-msi
    simulated:
      - code-signing
  mas:
    platform: macos
    arch: universal
    distribution: mac-app-store
    packaging: mas-pkg
    simulated:
      - signing
      - notarization

packaging:
  macos-dmg:
    workflow: release-target.yml
    inputs:
      package: dmg
  windows-msi:
    workflow: release-target.yml
    inputs:
      package: msi
  mas-pkg:
    workflow: release-target.yml
    inputs:
      package: pkg
";

fn token() -> SecretToken {
    SecretToken::new("gho_test_token")
}

async fn board() -> ReleaseBoard {
    let store = Store::in_memory().await.unwrap();
    let mut board = ReleaseBoard::new(store);
    board.enter("octo/alpha");
    board
}

fn asset(name: &str) -> ReleaseAsset {
    ReleaseAsset {
        name: name.to_owned(),
        size_in_bytes: 1024,
        download_url: Some(format!("https://example.test/{name}")),
        created_at: Some("2026-09-22T10:00:00Z".to_owned()),
    }
}

fn run(id: u64, status: RunStatus, conclusion: Option<&str>, created_at: &str) -> WorkflowRun {
    WorkflowRun {
        id,
        workflow_id: 1,
        name: "release-target".to_owned(),
        status,
        conclusion: conclusion.map(str::to_owned),
        branch: Some("main".to_owned()),
        event: "workflow_dispatch".to_owned(),
        actor: Some("octocat".to_owned()),
        created_at: Some(created_at.to_owned()),
        html_url: None,
    }
}

/// 一个 tag push 触发的运行：`head_branch` 就是 tag 名——看板靠它认出"这个 tag 现在
/// 指着哪次构建"。
fn tagged_run(id: u64, tag: &str, status: RunStatus, conclusion: Option<&str>) -> WorkflowRun {
    WorkflowRun {
        branch: Some(tag.to_owned()),
        event: "push".to_owned(),
        ..run(id, status, conclusion, "2999-01-01T00:00:00Z")
    }
}

fn commit(sha: &str, message: &str) -> CommitSummary {
    CommitSummary {
        message: message.to_owned(),
        sha: sha.to_owned(),
        author: Some("octocat".to_owned()),
        committed_at: Some("2026-09-27T10:00:00Z".to_owned()),
    }
}

fn job(id: u64, name: &str, status: RunStatus, conclusion: Option<&str>) -> Job {
    Job {
        id,
        name: name.to_owned(),
        status,
        conclusion: conclusion.map(str::to_owned),
        steps: Vec::new(),
    }
}

/// 通道 tag 建在选中的提交上——这就是"发布到这条通道"。
#[tokio::test]
async fn a_channel_tag_is_created_on_the_picked_commit() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest(MANIFEST);
    gateway.push_tag(Ok(()));
    let mut board = board().await;
    board.load_manifest(&*gateway, &token()).await;

    board
        .create_tag(&*gateway, &token(), "latest", "abc123")
        .await
        .unwrap();

    assert_eq!(
        gateway.tags(),
        [("latest".to_owned(), "abc123".to_owned())],
        "the tag was not pointed at the picked commit"
    );
}

/// 建 tag 失败时说得出是网关的问题。
#[tokio::test]
async fn a_tag_the_gateway_refuses_is_surfaced() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest(MANIFEST);
    gateway.push_tag(Err(GatewayError::Forbidden));
    let mut board = board().await;
    board.load_manifest(&*gateway, &token()).await;

    let outcome = board.create_tag(&*gateway, &token(), "lts", "abc123").await;

    assert_eq!(outcome, Err(TagProblem::Gateway(AppProblem::Forbidden)));
}

/// 没挑提交时，先读这条分支上的提交，挑最新那个当 tag 的目标。
#[tokio::test]
async fn the_branch_head_answers_when_no_commit_was_picked() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest(MANIFEST);
    gateway.push_commits(vec![commit("head1", "发布 v0.2.0")]);
    let mut board = board().await;
    board.load_manifest(&*gateway, &token()).await;

    let commits = board
        .branch_commits(&*gateway, &token(), "main", 1)
        .await
        .unwrap();

    assert_eq!(commits[0].sha, "head1");
    assert_eq!(gateway.commit_reads(), [("main".to_owned(), 1)]);
}

/// 格子里的状态来自这个 tag 那次运行里属于该目标的 job。
#[tokio::test]
async fn a_cell_shows_the_targets_job_in_the_tags_run() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest(MANIFEST);
    gateway.push_jobs(vec![
        job(
            1,
            "web-arm · macos/arm64 · dmg",
            RunStatus::Completed,
            Some("success"),
        ),
        job(
            2,
            "windows · windows/x64 · msi",
            RunStatus::Completed,
            Some("failure"),
        ),
        job(
            3,
            "mas · macos/universal · pkg",
            RunStatus::Completed,
            Some("success"),
        ),
    ]);
    let mut board = board().await;
    board.load_manifest(&*gateway, &token()).await;

    let runs = vec![tagged_run(
        321,
        "latest",
        RunStatus::Completed,
        Some("success"),
    )];
    // tag 那次运行的 job 还没取：视图会去取。
    assert_eq!(board.tag_jobs_to_load(&runs), [("latest".to_owned(), 321)]);
    board.begin_tag_jobs("latest", 321);
    board
        .load_tag_jobs(&*gateway, &token(), "latest", 321)
        .await;
    assert_eq!(gateway.job_reads(), [321]);
    // 取过了就不用再取。
    assert!(board.tag_jobs_to_load(&runs).is_empty());

    let snapshot = board.snapshot(&runs);
    let cell = |target: &str, tag: &str| {
        snapshot
            .rows
            .iter()
            .find(|row| row.target == target)
            .and_then(|row| row.cells.iter().find(|cell| cell.channel == tag))
            .cloned()
            .unwrap_or_else(|| panic!("no cell for {target}/{tag}"))
    };

    // 成功的 job：这个通道 tag 就发布到了这个目标上。
    let arm = cell("web-arm", "latest");
    assert_eq!(arm.state, ReleaseState::Published);
    assert_eq!(arm.job.as_deref(), Some("web-arm · macos/arm64 · dmg"));
    assert_eq!(arm.run_id, Some(321));
    // 失败的 job：这一格是失败。
    assert_eq!(cell("windows", "latest").state, ReleaseState::Failed);
    // 带模拟步骤的目标停在"待签名公证"。
    assert_eq!(cell("mas", "latest").state, ReleaseState::AwaitingSigning);
    // 这个 tag 没跑过的那条：待构建，也没有 job 和运行可看。
    let lts = cell("web-arm", "lts");
    assert_eq!(lts.state, ReleaseState::Todo);
    assert!(lts.job.is_none());
    assert!(lts.run_id.is_none());
}

#[tokio::test]
async fn a_manifest_becomes_targets_and_recipes() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest(MANIFEST);
    let mut board = board().await;

    board.load_manifest(&*gateway, &token()).await;

    assert_eq!(board.manifest_state(), ManifestState::Loaded);
    let manifest = board.manifest().unwrap();
    assert_eq!(manifest.targets().len(), 4);
    let (target, config) = manifest.recipe_for("mas").unwrap();
    assert!(target.is_simulated());
    assert_eq!(config.workflow, "release-target.yml");
    assert_eq!(config.input("package"), Some("pkg"));
}

#[tokio::test]
async fn a_repository_without_a_manifest_is_a_state_of_its_own() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_missing_manifest();
    let mut board = board().await;

    board.load_manifest(&*gateway, &token()).await;

    assert_eq!(board.manifest_state(), ManifestState::Missing);
    assert!(board.manifest().is_none());
}

#[tokio::test]
async fn a_manifest_the_console_cannot_read_says_why() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest("targets:\n  web arm:\n    platform: macos\n");
    let mut board = board().await;

    board.load_manifest(&*gateway, &token()).await;

    assert_eq!(board.manifest_state(), ManifestState::Invalid);
    assert!(
        board
            .manifest_problem()
            .unwrap()
            .to_string()
            .contains("web arm")
    );
}

#[tokio::test]
async fn triggering_a_target_sends_the_recipe_and_records_the_ask() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest(MANIFEST);
    gateway.push_dispatch(Ok(()));
    let mut board = board().await;
    board.load_manifest(&*gateway, &token()).await;

    let dispatch = board
        .trigger(&*gateway, &token(), "web-arm", "v0.1.0", None)
        .await
        .expect("the target has a recipe");

    let sent = gateway.dispatches();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].workflow, "release-target.yml");
    assert_eq!(sent[0].reference, "v0.1.0");
    assert!(
        sent[0]
            .inputs
            .contains(&("target".to_owned(), "web-arm".to_owned()))
    );
    assert!(
        sent[0]
            .inputs
            .contains(&("version".to_owned(), "v0.1.0".to_owned()))
    );
    assert!(
        sent[0]
            .inputs
            .contains(&("config".to_owned(), "macos-dmg".to_owned()))
    );
    assert!(
        sent[0]
            .inputs
            .contains(&("package".to_owned(), "dmg".to_owned()))
    );
    assert_eq!(dispatch.target, "web-arm");
    assert_eq!(dispatch.run_id, None);
}

#[tokio::test]
async fn a_target_the_manifest_does_not_define_is_never_dispatched() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest(MANIFEST);
    let mut board = board().await;
    board.load_manifest(&*gateway, &token()).await;

    let outcome = board
        .trigger(&*gateway, &token(), "linux-tar", "v0.1.0", None)
        .await;

    assert_eq!(outcome, Err(TriggerProblem::UnknownTarget));
    assert!(gateway.dispatches().is_empty());
}

#[tokio::test]
async fn a_dispatch_is_bound_to_the_run_it_became() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest(MANIFEST);
    gateway.push_dispatch(Ok(()));
    let mut board = board().await;
    board.load_manifest(&*gateway, &token()).await;
    board
        .trigger(&*gateway, &token(), "web-arm", "v0.1.0", None)
        .await
        .unwrap();

    let runs = vec![run(77, RunStatus::InProgress, None, "2999-01-01T00:00:00Z")];
    board.bind_dispatches(&runs).await;

    assert_eq!(board.dispatches()[0].run_id, Some(77));
    let manifest = board.manifest().unwrap().clone();
    let target = manifest.target("web-arm").unwrap();
    let cell = board.cell(target, "latest", &runs);
    assert_eq!(cell.state, ReleaseState::Building);
    assert_eq!(cell.run_id, Some(77));
}

#[tokio::test]
async fn a_green_build_is_not_a_release() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest(MANIFEST);
    gateway.push_dispatch(Ok(()));
    gateway.push_dispatch(Ok(()));
    let mut board = board().await;
    board.load_manifest(&*gateway, &token()).await;
    board
        .trigger(&*gateway, &token(), "web-arm", "v0.1.0", None)
        .await
        .unwrap();
    board
        .trigger(&*gateway, &token(), "mas", "v0.1.0", None)
        .await
        .unwrap();
    let runs = vec![
        run(
            88,
            RunStatus::Completed,
            Some("success"),
            "2999-01-01T00:00:00Z",
        ),
        run(
            89,
            RunStatus::Completed,
            Some("success"),
            "2999-01-01T00:00:01Z",
        ),
    ];
    board.bind_dispatches(&runs).await;

    let manifest = board.manifest().unwrap().clone();
    let arm = manifest.target("web-arm").unwrap();
    let mas = manifest.target("mas").unwrap();

    // Built, waiting for the explicit publish action…
    assert_eq!(
        board.cell(arm, "latest", &runs).state,
        ReleaseState::ReadyToPublish
    );
    // …and a target with simulated steps stops short of "已发布".
    assert_eq!(
        board.cell(mas, "latest", &runs).state,
        ReleaseState::AwaitingSigning
    );
}

#[tokio::test]
async fn publishing_needs_an_asset_for_that_target() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest(MANIFEST);
    // The release exists, but only carries the mas package.
    gateway.push_assets(vec![asset("github-action-console-0.1.0-mas-unsigned.pkg")]);
    let mut board = board().await;
    board.load_manifest(&*gateway, &token()).await;

    let outcome = board
        .publish(&*gateway, &token(), "web-arm", "latest", "v0.1.0")
        .await;

    assert_eq!(outcome, Err(PublishProblem::NothingToPublish));
    assert_eq!(gateway.release_lookups(), ["v0.1.0"]);
    assert!(board.pointers().is_empty(), "nothing may be published");
}

#[tokio::test]
async fn publishing_writes_one_pointer_and_leaves_the_others_alone() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest(MANIFEST);
    gateway.push_assets(vec![asset("github-action-console-0.1.0-web-arm.dmg")]);
    let mut board = board().await;
    board.load_manifest(&*gateway, &token()).await;

    // Publish web-arm → latest, then web-arm → lts.
    let published = board
        .publish(&*gateway, &token(), "web-arm", "latest", "v0.1.0")
        .await
        .unwrap();
    assert_eq!(published.channel, "latest");
    gateway.push_assets(vec![asset("github-action-console-0.1.0-web-arm.dmg")]);
    board
        .publish(&*gateway, &token(), "web-arm", "lts", "v0.1.0")
        .await
        .unwrap();

    assert_eq!(board.pointers().len(), 2);
    assert_eq!(
        board.pointer("web-arm", "latest").unwrap().version,
        "v0.1.0"
    );
    assert_eq!(board.pointer("web-arm", "lts").unwrap().version, "v0.1.0");
    // Every other (target, channel) cell is still empty: nothing else moved.
    for target in ["web-intel", "windows", "mas"] {
        for channel in CHANNELS {
            assert!(
                board.pointer(target, channel).is_none(),
                "{target}/{channel} should still be empty"
            );
        }
    }
}

#[tokio::test]
async fn a_failed_build_on_one_target_does_not_touch_another_targets_release() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest(MANIFEST);
    gateway.push_assets(vec![asset("github-action-console-0.1.0-web-arm.dmg")]);
    gateway.push_dispatch(Ok(()));
    let mut board = board().await;
    board.load_manifest(&*gateway, &token()).await;
    board
        .publish(&*gateway, &token(), "web-arm", "latest", "v0.1.0")
        .await
        .unwrap();

    // windows fails: the dispatch is recorded, its run comes back red.
    board
        .trigger(&*gateway, &token(), "windows", "v0.2.0", None)
        .await
        .unwrap();
    let runs = vec![run(
        99,
        RunStatus::Completed,
        Some("failure"),
        "2999-01-01T00:00:00Z",
    )];
    board.bind_dispatches(&runs).await;

    let manifest = board.manifest().unwrap().clone();
    let windows = manifest.target("windows").unwrap();
    let arm = manifest.target("web-arm").unwrap();
    assert_eq!(
        board.cell(windows, "latest", &runs).state,
        ReleaseState::Failed
    );
    // The published web-arm pointer is untouched.
    assert_eq!(
        board.pointer("web-arm", "latest").unwrap().version,
        "v0.1.0"
    );
    assert_eq!(
        board.cell(arm, "latest", &runs).state,
        ReleaseState::Published
    );
}

#[tokio::test]
async fn pointers_survive_a_restart() {
    let store = Store::in_memory().await.unwrap();
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest(MANIFEST);
    gateway.push_assets(vec![asset("github-action-console-0.1.0-web-arm.dmg")]);

    let mut board = ReleaseBoard::new(store.clone());
    board.enter("octo/alpha");
    board.load_manifest(&*gateway, &token()).await;
    board
        .publish(&*gateway, &token(), "web-arm", "latest", "v0.1.0")
        .await
        .unwrap();

    // A fresh board over the same store knows the pointer.
    let mut restarted = ReleaseBoard::new(store);
    restarted.enter("octo/alpha");
    restarted.load_pointers().await;

    assert_eq!(
        restarted.pointer("web-arm", "latest").unwrap().version,
        "v0.1.0"
    );
    assert!(restarted.pointer("mas", "latest").is_none());
}

#[tokio::test]
async fn a_run_reports_its_three_facts_separately() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest(MANIFEST);
    gateway.push_dispatch(Ok(()));
    // 构建产物已经有了：这个版本带着 web-arm 的资产。
    gateway.push_assets(vec![
        asset("github-action-console-0.1.0-web-arm.dmg"),
        asset("github-action-console-0.1.0-mas-unsigned.pkg"),
    ]);
    let mut board = board().await;
    board.load_manifest(&*gateway, &token()).await;
    board
        .trigger(&*gateway, &token(), "web-arm", "v0.1.0", None)
        .await
        .unwrap();
    let runs = vec![run(
        11,
        RunStatus::Completed,
        Some("success"),
        "2999-01-01T00:00:00Z",
    )];
    board.bind_dispatches(&runs).await;

    let facts = board
        .facts_for_run(&*gateway, &token(), 11)
        .await
        .expect("the console asked for this run");

    // 谁产出的：版本、目标、配置。
    assert_eq!(facts.version, "v0.1.0");
    assert_eq!(facts.target, "web-arm");
    assert_eq!(facts.config, "macos-dmg");
    // 产物可获取：只有这个目标的资产，别的目标的包不算。
    assert_eq!(facts.assets.len(), 1);
    assert_eq!(
        facts.assets[0].name,
        "github-action-console-0.1.0-web-arm.dmg"
    );
    assert!(facts.assets[0].created_at.is_some());
    // 已登记发布：还没发布，所以没有指针。
    assert!(facts.pointers.is_empty());
    // 看板那一格此时是"待发布"——三个事实确实不同。
    let manifest = board.manifest().unwrap().clone();
    assert_eq!(
        board
            .cell(manifest.target("web-arm").unwrap(), "latest", &runs)
            .state,
        ReleaseState::ReadyToPublish
    );

    // 发布之后，同一个 run 的第三条事实变成真的。
    gateway.push_assets(vec![asset("github-action-console-0.1.0-web-arm.dmg")]);
    board
        .publish(&*gateway, &token(), "web-arm", "latest", "v0.1.0")
        .await
        .unwrap();
    let facts = board.facts_for_run(&*gateway, &token(), 11).await.unwrap();
    assert_eq!(facts.pointers.len(), 1);
    assert_eq!(facts.pointers[0].channel, "latest");
    assert_eq!(facts.pointers[0].version, "v0.1.0");
}

#[tokio::test]
async fn a_run_the_console_did_not_start_has_no_facts() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest(MANIFEST);
    let mut board = board().await;
    board.load_manifest(&*gateway, &token()).await;

    assert!(
        board
            .facts_for_run(&*gateway, &token(), 999)
            .await
            .is_none()
    );
}

#[tokio::test]
async fn a_manifest_write_opens_a_pull_request_and_never_touches_the_default_branch() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest(MANIFEST);
    let mut board = board().await;
    board.load_manifest(&*gateway, &token()).await;
    // 写回时还要再读一次现在的 revision。
    gateway.push_manifest(MANIFEST);

    let written = board
        .save_manifest(&*gateway, &token(), "main", "targets: {}\n")
        .await
        .expect("the write goes through");

    // 分支从默认分支拉出来，不直接写默认分支。
    let branches = gateway.branches();
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0].1, "main");
    assert!(branches[0].0.starts_with("release-console/manifest-"));

    let writes = gateway.writes();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].path, ".github/release-console.yml");
    assert_eq!(writes[0].reference, branches[0].0, "写到新分支上");
    assert!(writes[0].reference != "main");
    assert_eq!(writes[0].sha.as_deref(), Some("sha-manifest"));
    assert!(writes[0].message.contains("更新"));

    // 开成 PR，base 是默认分支。
    let pulls = gateway.pulls();
    assert_eq!(pulls.len(), 1);
    assert_eq!(pulls[0].0, branches[0].0);
    assert_eq!(pulls[0].1, "main");
    assert_eq!(written.pull_number, 42);
    assert_eq!(written.branch, branches[0].0);
}

#[tokio::test]
async fn a_repository_without_a_manifest_gets_its_first_one_the_same_way() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_missing_manifest();
    let mut board = board().await;
    board.load_manifest(&*gateway, &token()).await;
    gateway.push_missing_manifest();

    board
        .save_manifest(&*gateway, &token(), "main", "targets: {}\n")
        .await
        .expect("a first manifest is still a pull request");

    let writes = gateway.writes();
    assert_eq!(writes.len(), 1);
    // 新建文件没有 revision 可基于。
    assert_eq!(writes[0].sha, None);
    assert!(writes[0].message.contains("添加"));
    assert_eq!(gateway.pulls().len(), 1);
}

#[test]
fn an_asset_belongs_to_the_target_its_name_carries() {
    let assets = vec![
        asset("github-action-console-0.1.0-web-arm.dmg"),
        asset("github-action-console-0.1.0-mas-unsigned.pkg"),
    ];

    assert!(version_covers_target(&assets, "web-arm"));
    assert!(version_covers_target(&assets, "mas"));
    assert!(!version_covers_target(&assets, "windows"));
    assert!(!version_covers_target(&[], "web-arm"));
}

#[test]
fn a_gateway_failure_while_reading_assets_is_a_gateway_problem() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_manifest(MANIFEST);
    // 第一个版本根本没有对应的 release：404 是"没有可发布的产物"，不是网络错误。
    gateway.push_missing_release();
    // 第二个版本查资产时被 GitHub 拒绝。
    gateway
        .assets
        .lock()
        .unwrap()
        .push_back(Err(GatewayError::Forbidden));

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (missing, refused) = runtime.block_on(async {
        let mut board = board().await;
        board.load_manifest(&*gateway, &token()).await;
        let missing = board
            .publish(&*gateway, &token(), "web-arm", "latest", "v0.9.9")
            .await;
        let refused = board
            .publish(&*gateway, &token(), "web-arm", "latest", "v0.1.0")
            .await;
        (missing, refused)
    });

    assert_eq!(missing, Err(PublishProblem::NothingToPublish));
    assert_eq!(refused, Err(PublishProblem::Gateway(AppProblem::Forbidden)));
}
