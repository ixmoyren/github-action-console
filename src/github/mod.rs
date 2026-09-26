pub mod client;

use async_trait::async_trait;
use thiserror::Error;

/// The identity the console is currently acting as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub login: String,
}

/// A GitHub access token. Never rendered or logged.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretToken(String);

impl SecretToken {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SecretToken(***)")
    }
}

/// Opaque handle to an in-flight device-flow authorization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceFlowHandle(String);

impl DeviceFlowHandle {
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    pub fn code(&self) -> &str {
        &self.0
    }
}

/// What the user needs in order to authorize this device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceFlowStart {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in_secs: u64,
    pub interval_secs: u64,
    pub handle: DeviceFlowHandle,
}

/// The result of one poll against GitHub's device-flow token endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceFlowPoll {
    Pending,
    SlowDown,
    Authorized(SecretToken),
    Expired,
    Denied,
}

/// How repository listings are ordered by GitHub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositorySort {
    Updated,
    Pushed,
}

impl RepositorySort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Updated => "updated",
            Self::Pushed => "pushed",
        }
    }
}

/// The newest commit on a repository's default branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSummary {
    pub message: String,
    pub sha: String,
    pub author: Option<String>,
    pub committed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repository {
    pub name: String,
    pub full_name: String,
    pub is_private: bool,
    pub default_branch: Option<String>,
    pub latest_commit: Option<CommitSummary>,
}

impl Repository {
    /// Case-insensitive match against the repository name or full name.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        self.name.to_lowercase().contains(&query) || self.full_name.to_lowercase().contains(&query)
    }
}

/// The single name filter shared by the picker and its view.
pub fn filter_repositories(repositories: &[Repository], query: &str) -> Vec<Repository> {
    repositories
        .iter()
        .filter(|repository| repository.matches(query))
        .cloned()
        .collect()
}

/// One page of repositories plus whether another page exists.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RepositoryPage {
    pub repositories: Vec<Repository>,
    pub has_more: bool,
}

/// A workflow defined in a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workflow {
    pub id: u64,
    pub name: String,
    pub path: String,
}

/// A repository file as GitHub currently holds it. The SHA is the revision a
/// later write has to name, so two writers cannot silently overwrite one
/// another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileContents {
    pub text: String,
    pub sha: String,
}

/// A write to one repository file: the new text, the commit that carries it,
/// and the revision the write starts from. A write without a revision creates
/// the file instead of replacing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileWrite {
    pub path: String,
    pub contents: String,
    pub message: String,
    pub reference: String,
    pub sha: Option<String>,
}

/// A pull request to open: what it says, where it comes from, where it goes.
pub struct PullRequest {
    pub title: String,
    pub body: String,
    pub head: String,
    pub base: String,
}

/// GitHub's coarse run status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Queued,
    InProgress,
    Completed,
    Unknown,
}

impl RunStatus {
    pub fn parse(value: &str) -> Self {
        match value {
            "queued" | "requested" | "pending" | "waiting" => Self::Queued,
            "in_progress" => Self::InProgress,
            "completed" => Self::Completed,
            _ => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "排队中",
            Self::InProgress => "进行中",
            Self::Completed => "已完成",
            Self::Unknown => "未知",
        }
    }

    /// Whether the run is still moving, i.e. worth polling for.
    pub fn is_running(self) -> bool {
        matches!(self, Self::Queued | Self::InProgress)
    }
}

/// One workflow run, reduced to what the console shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowRun {
    pub id: u64,
    pub workflow_id: u64,
    pub name: String,
    pub status: RunStatus,
    pub conclusion: Option<String>,
    pub branch: Option<String>,
    pub event: String,
    pub actor: Option<String>,
    pub created_at: Option<String>,
    pub html_url: Option<String>,
}

/// One page of runs plus whether another page exists.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkflowRunPage {
    pub runs: Vec<WorkflowRun>,
    pub has_more: bool,
}

/// One step inside a job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub number: i64,
    pub name: String,
    pub status: RunStatus,
    pub conclusion: Option<String>,
}

/// One job inside a run, with its steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: u64,
    pub name: String,
    pub status: RunStatus,
    pub conclusion: Option<String>,
    pub steps: Vec<Step>,
}

/// The account's current core rate-limit budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimit {
    pub limit: u64,
    pub remaining: u64,
    pub reset_at: Option<String>,
}

/// A build artifact produced by a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildArtifact {
    pub id: u64,
    pub name: String,
    pub size_in_bytes: u64,
    pub expired: bool,
    pub download_url: Option<String>,
}

/// A file attached to a release: what a published version actually carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub size_in_bytes: u64,
    pub download_url: Option<String>,
    /// When the asset was attached: the moment "已发布" became true.
    pub created_at: Option<String>,
}

/// Keep only the log lines matching `query` (case-insensitive). An empty query
/// returns the log unchanged.
pub fn filter_log_lines(log: &str, query: &str) -> String {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return log.to_owned();
    }
    log.lines()
        .filter(|line| line.to_lowercase().contains(&query))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Status buckets the runs view can filter on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunStatusFilter {
    #[default]
    All,
    Running,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunFilter {
    pub status: RunStatusFilter,
    pub branch: Option<String>,
    pub workflow_id: Option<u64>,
}

impl RunFilter {
    pub fn matches(&self, run: &WorkflowRun) -> bool {
        let status_ok = match self.status {
            RunStatusFilter::All => true,
            RunStatusFilter::Running => run.status.is_running(),
            RunStatusFilter::Completed => !run.status.is_running(),
        };
        let branch_ok = match &self.branch {
            None => true,
            Some(branch) => run.branch.as_deref() == Some(branch.as_str()),
        };
        let workflow_ok = match self.workflow_id {
            None => true,
            Some(workflow_id) => run.workflow_id == workflow_id,
        };
        status_ok && branch_ok && workflow_ok
    }
}

/// The single run filter shared by the runs view and its tests.
pub fn filter_runs(runs: &[WorkflowRun], filter: &RunFilter) -> Vec<WorkflowRun> {
    runs.iter()
        .filter(|run| filter.matches(run))
        .cloned()
        .collect()
}

/// Whether any run is still moving.
pub fn any_running(runs: &[WorkflowRun]) -> bool {
    runs.iter().any(|run| run.status.is_running())
}

/// Split an `owner/name` repository into its two halves.
pub fn split_full_name(full_name: &str) -> Option<(String, String)> {
    let (owner, name) = full_name.split_once('/')?;
    if owner.is_empty() || name.is_empty() {
        return None;
    }
    Some((owner.to_owned(), name.to_owned()))
}

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("device flow is not configured")]
    DeviceFlowUnavailable,
    #[error("credentials are no longer valid")]
    Unauthorized,
    #[error("insufficient permissions")]
    Forbidden,
    #[error("rate limited")]
    RateLimited,
    #[error("not found")]
    NotFound,
    #[error("transport failure: {0}")]
    Transport(String),
    #[error("unexpected response: {0}")]
    Unexpected(String),
}

/// The single seam between the application and GitHub.
#[async_trait]
pub trait GitHubGateway: Send + Sync {
    /// Begin a device-flow authorization for the given OAuth App client id.
    async fn start_device_flow(&self, client_id: &str) -> Result<DeviceFlowStart, GatewayError>;

    async fn poll_device_flow(
        &self,
        client_id: &str,
        handle: &DeviceFlowHandle,
    ) -> Result<DeviceFlowPoll, GatewayError>;

    async fn current_user(&self, token: &SecretToken) -> Result<Account, GatewayError>;

    async fn list_repositories(
        &self,
        token: &SecretToken,
        sort: RepositorySort,
        page: u32,
        per_page: u32,
    ) -> Result<RepositoryPage, GatewayError>;

    async fn list_workflows(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
    ) -> Result<Vec<Workflow>, GatewayError>;

    /// The labels a repository's self-hosted runners answer to. GitHub has no
    /// equivalent list for its hosted images, so those come from the console.
    async fn runner_labels(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
    ) -> Result<Vec<String>, GatewayError>;

    /// One file in the repository, e.g. the YAML of a workflow. `path` is
    /// repository-relative, like `.github/workflows/ci.yml`.
    async fn file_contents(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        path: &str,
    ) -> Result<FileContents, GatewayError>;

    /// Create or replace a file on `reference`, as one commit: replacing names
    /// the revision it starts from, creating leaves it empty.
    async fn write_file(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        write: FileWrite,
    ) -> Result<(), GatewayError>;

    /// Create a branch whose head is where `from` points right now.
    async fn create_branch(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        branch: &str,
        from: &str,
    ) -> Result<(), GatewayError>;

    /// Open a pull request, returning its number.
    async fn open_pull_request(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        pull: PullRequest,
    ) -> Result<u64, GatewayError>;

    /// Trigger a `workflow_dispatch` run of `workflow` — its file name or its
    /// id — on `reference`, with the given inputs.
    async fn dispatch_workflow(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        workflow: &str,
        reference: &str,
        inputs: &[(String, String)],
    ) -> Result<(), GatewayError>;

    /// All runs in the repository, or the runs of one workflow when
    /// `workflow_id` is set.
    async fn list_workflow_runs(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        workflow_id: Option<u64>,
        page: u32,
        per_page: u32,
    ) -> Result<WorkflowRunPage, GatewayError>;

    async fn list_jobs(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        run_id: u64,
    ) -> Result<Vec<Job>, GatewayError>;

    /// A job's raw log text. An empty string is a valid, non-error result.
    async fn job_logs(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        job_id: u64,
    ) -> Result<String, GatewayError>;

    /// The zip archive of a whole run's logs.
    async fn run_logs_archive(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        run_id: u64,
    ) -> Result<Vec<u8>, GatewayError>;

    async fn list_artifacts(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        run_id: u64,
    ) -> Result<Vec<BuildArtifact>, GatewayError>;

    /// One artifact's zip archive.
    async fn download_artifact(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        artifact_id: u64,
    ) -> Result<Vec<u8>, GatewayError>;

    /// The assets a release carries, by tag. A tag without a release is
    /// `NotFound`, which is a fact about publishing, not a failure.
    async fn release_assets(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        tag: &str,
    ) -> Result<Vec<ReleaseAsset>, GatewayError>;

    /// The core rate-limit budget for the current credentials.
    async fn rate_limit(&self, token: &SecretToken) -> Result<RateLimit, GatewayError>;

    /// Route every later request through this proxy (or none when `None`).
    fn set_proxy(&self, proxy: Option<String>);
}
