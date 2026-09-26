use octocrab::Octocrab;
use octocrab::auth::DeviceCodes;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use super::{
    Account, BuildArtifact, CommitSummary, DeviceFlowHandle, DeviceFlowPoll, DeviceFlowStart,
    FileContents, FileWrite, GatewayError, GitHubGateway, Job, RateLimit, Repository,
    RepositoryPage, RepositorySort, RunStatus, SecretToken, Step, Workflow, WorkflowRun,
    WorkflowRunPage,
};

const DEFAULT_BASE_URI: &str = "https://github.com";

impl Default for OctocrabGateway {
    fn default() -> Self {
        Self::new()
    }
}
const DEVICE_FLOW_SCOPES: [&str; 2] = ["repo", "workflow"];
const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// octocrab-backed gateway. Thin mapping only: no retry policy, no caching.
pub struct OctocrabGateway {
    base_uri: String,
}

impl OctocrabGateway {
    pub fn new() -> Self {
        Self {
            base_uri: DEFAULT_BASE_URI.to_owned(),
        }
    }

    /// Device flow must talk to the web host and ask for JSON.
    fn device_flow_client(&self) -> Result<Octocrab, GatewayError> {
        Octocrab::builder()
            .base_uri(&self.base_uri)
            .map_err(unexpected)?
            .add_header(http::header::ACCEPT, "application/json".to_owned())
            .build()
            .map_err(unexpected)
    }
}

#[async_trait::async_trait]
impl GitHubGateway for OctocrabGateway {
    async fn start_device_flow(&self, client_id: &str) -> Result<DeviceFlowStart, GatewayError> {
        let crab = self.device_flow_client()?;
        let codes: DeviceCodes = crab
            .authenticate_as_device(
                &SecretString::from(client_id.to_owned()),
                DEVICE_FLOW_SCOPES,
            )
            .await
            .map_err(map_error)?;

        Ok(DeviceFlowStart {
            user_code: codes.user_code,
            verification_uri: codes.verification_uri,
            expires_in_secs: codes.expires_in,
            interval_secs: codes.interval,
            handle: DeviceFlowHandle::new(codes.device_code),
        })
    }

    async fn poll_device_flow(
        &self,
        client_id: &str,
        handle: &DeviceFlowHandle,
    ) -> Result<DeviceFlowPoll, GatewayError> {
        let crab = self.device_flow_client()?;
        let body = PollBody {
            client_id,
            device_code: handle.code(),
            grant_type: DEVICE_GRANT_TYPE,
        };

        // GitHub answers 200 with either `access_token` or `error`, so the
        // response is decoded here rather than through octocrab's typed
        // `TokenResponse`, which cannot represent `expired_token`/`access_denied`.
        let response: PollResponse = crab
            .post("/login/oauth/access_token", Some(&body))
            .await
            .map_err(map_error)?;

        if let Some(token) = response.access_token {
            return Ok(DeviceFlowPoll::Authorized(SecretToken::new(token)));
        }

        match response.error.as_deref() {
            Some("authorization_pending") => Ok(DeviceFlowPoll::Pending),
            Some("slow_down") => Ok(DeviceFlowPoll::SlowDown),
            Some("expired_token") => Ok(DeviceFlowPoll::Expired),
            Some("access_denied") => Ok(DeviceFlowPoll::Denied),
            Some(other) => Err(GatewayError::Unexpected(other.to_owned())),
            None => Err(GatewayError::Unexpected(
                "empty device flow response".to_owned(),
            )),
        }
    }

    async fn current_user(&self, token: &SecretToken) -> Result<Account, GatewayError> {
        let crab = user_client(token)?;
        let author = crab.current().user().await.map_err(map_error)?;
        Ok(Account {
            login: author.login,
        })
    }

    async fn list_repositories(
        &self,
        token: &SecretToken,
        sort: RepositorySort,
        page: u32,
        per_page: u32,
    ) -> Result<RepositoryPage, GatewayError> {
        let crab = user_client(token)?;
        let page = crab
            .current()
            .list_repos_for_authenticated_user()
            // GitHub rejects mixing `visibility`/`affiliation` with `type`,
            // and the affiliation list already covers every repo we can see.
            .visibility("all")
            .affiliation("owner,collaborator,organization_member")
            .sort(sort.as_str())
            .direction("desc")
            .per_page(per_page.clamp(1, 100) as u8)
            .page(page.clamp(1, 255) as u8)
            .send()
            .await
            .map_err(map_error)?;

        // The listing has no commit message, so each repository needs one
        // extra request for its default branch's newest commit. They run
        // together; a single failure only blanks that row's commit columns.
        let has_more = page.next.is_some();
        let repositories =
            futures_util::future::join_all(page.items.into_iter().map(|repository| {
                let crab = crab.clone();
                async move {
                    let full_name = repository.full_name.unwrap_or_default();
                    let latest_commit = latest_commit(&crab, &full_name).await;
                    Repository {
                        name: repository.name,
                        full_name,
                        is_private: repository.private.unwrap_or(false),
                        default_branch: repository.default_branch,
                        latest_commit,
                    }
                }
            }))
            .await;

        Ok(RepositoryPage {
            repositories,
            has_more,
        })
    }

    async fn list_workflows(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
    ) -> Result<Vec<Workflow>, GatewayError> {
        let crab = user_client(token)?;
        let page = crab
            .workflows(owner, repository)
            .list()
            .per_page(100u8)
            .send()
            .await
            .map_err(map_error)?;

        Ok(page
            .items
            .into_iter()
            .map(|workflow| Workflow {
                id: workflow.id.into_inner(),
                name: workflow.name,
                path: workflow.path,
            })
            .collect())
    }

    async fn file_contents(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        path: &str,
    ) -> Result<FileContents, GatewayError> {
        let crab = user_client(token)?;
        let mut items = crab
            .repos(owner, repository)
            .get_content()
            .path(path)
            .send()
            .await
            .map_err(map_error)?;

        // GitHub answers with an array for a directory and a single object for
        // a file; only the latter carries decodable content.
        items
            .take_items()
            .into_iter()
            .find(|content| content.r#type == "file")
            .and_then(|content| {
                content.decoded_content().map(|text| FileContents {
                    text,
                    sha: content.sha.clone(),
                })
            })
            .ok_or_else(|| GatewayError::Unexpected(format!("{path} is not a file")))
    }

    async fn update_file(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        write: FileWrite,
    ) -> Result<(), GatewayError> {
        let crab = user_client(token)?;
        crab.repos(owner, repository)
            .update_file(write.path, write.message, write.contents, write.sha)
            .branch(write.reference)
            .send()
            .await
            .map_err(map_error)?;

        Ok(())
    }

    async fn dispatch_workflow(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        workflow_id: u64,
        reference: &str,
    ) -> Result<(), GatewayError> {
        let crab = user_client(token)?;
        crab.actions()
            .create_workflow_dispatch(owner, repository, workflow_id.to_string(), reference)
            .send()
            .await
            .map_err(map_error)
    }

    async fn list_workflow_runs(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        workflow_id: Option<u64>,
        page: u32,
        per_page: u32,
    ) -> Result<WorkflowRunPage, GatewayError> {
        let crab = user_client(token)?;
        let handler = crab.workflows(owner, repository);
        let builder = match workflow_id {
            Some(id) => handler.list_runs(id.to_string()),
            None => handler.list_all_runs(),
        };

        let page_number = page.max(1);
        let page = builder
            .per_page(per_page.clamp(1, 100) as u8)
            .page(page_number)
            .send()
            .await
            .map_err(map_error)?;

        Ok(WorkflowRunPage {
            runs: page.items.into_iter().map(map_run).collect(),
            has_more: page.next.is_some(),
        })
    }

    async fn list_jobs(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        run_id: u64,
    ) -> Result<Vec<Job>, GatewayError> {
        let crab = user_client(token)?;
        let page = crab
            .workflows(owner, repository)
            .list_jobs(octocrab::models::RunId::from(run_id))
            .per_page(100u8)
            .send()
            .await
            .map_err(map_error)?;

        Ok(page.items.into_iter().map(map_job).collect())
    }

    async fn job_logs(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        job_id: u64,
    ) -> Result<String, GatewayError> {
        let crab = user_client(token)?;
        let bytes = crab
            .workflows(owner, repository)
            .download_job_logs(octocrab::models::JobId::from(job_id))
            .await
            .map_err(map_error)?;

        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    async fn run_logs_archive(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        run_id: u64,
    ) -> Result<Vec<u8>, GatewayError> {
        let crab = user_client(token)?;
        let bytes = crab
            .actions()
            .download_workflow_run_logs(owner, repository, octocrab::models::RunId::from(run_id))
            .await
            .map_err(map_error)?;

        Ok(bytes.to_vec())
    }

    async fn list_artifacts(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        run_id: u64,
    ) -> Result<Vec<BuildArtifact>, GatewayError> {
        let crab = user_client(token)?;
        let etagged = crab
            .actions()
            .list_workflow_run_artifacts(owner, repository, octocrab::models::RunId::from(run_id))
            .send()
            .await
            .map_err(map_error)?;
        let page = etagged
            .value
            .ok_or_else(|| GatewayError::Unexpected("artifact list was not modified".to_owned()))?;

        Ok(page
            .items
            .into_iter()
            .map(|artifact| BuildArtifact {
                id: artifact.id.into_inner(),
                name: artifact.name,
                size_in_bytes: artifact.size_in_bytes as u64,
                expired: artifact.expired,
                download_url: Some(artifact.archive_download_url.to_string()),
            })
            .collect())
    }

    async fn download_artifact(
        &self,
        token: &SecretToken,
        owner: &str,
        repository: &str,
        artifact_id: u64,
    ) -> Result<Vec<u8>, GatewayError> {
        let crab = user_client(token)?;
        let bytes = crab
            .actions()
            .download_artifact(
                owner,
                repository,
                octocrab::models::ArtifactId::from(artifact_id),
                octocrab::params::actions::ArchiveFormat::Zip,
            )
            .await
            .map_err(map_error)?;

        Ok(bytes.to_vec())
    }

    fn set_proxy(&self, proxy: Option<String>) {
        for key in PROXY_ENV_KEYS {
            match proxy
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                // SAFETY: the console only writes these once per save, and every
                // request builds its own client, so the read happens either
                // before or after the write.
                Some(value) => unsafe { std::env::set_var(key, value) },
                None => unsafe { std::env::remove_var(key) },
            }
        }
    }

    async fn rate_limit(&self, token: &SecretToken) -> Result<RateLimit, GatewayError> {
        let crab = user_client(token)?;
        let limits = crab.ratelimit().get().await.map_err(map_error)?;
        let core = limits.resources.core;

        Ok(RateLimit {
            limit: core.limit as u64,
            remaining: core.remaining as u64,
            reset_at: chrono::DateTime::from_timestamp(core.reset as i64, 0)
                .map(|reset| reset.to_rfc3339()),
        })
    }
}

/// reqwest (and therefore octocrab) picks up proxies from the environment, so
/// the setting is applied there. Every call builds a fresh client, which is
/// what makes a change take effect without a restart.
const PROXY_ENV_KEYS: [&str; 3] = ["HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY"];

fn user_client(token: &SecretToken) -> Result<Octocrab, GatewayError> {
    Octocrab::builder()
        .user_access_token(token.expose().to_owned())
        .build()
        .map_err(unexpected)
}

/// The newest commit on `full_name`'s default branch, or `None` when the
/// repository is unreachable or has no commits yet.
async fn latest_commit(crab: &Octocrab, full_name: &str) -> Option<CommitSummary> {
    let (owner, repository) = full_name.split_once('/')?;
    let page = match crab
        .repos(owner, repository)
        .list_commits()
        .per_page(1u8)
        .send()
        .await
    {
        Ok(page) => page,
        Err(error) => {
            tracing::debug!(%full_name, %error, "could not read the latest commit");
            return None;
        }
    };

    page.items.into_iter().next().map(|commit| {
        let committed_at = commit
            .commit
            .author
            .as_ref()
            .and_then(|author| author.date)
            .or_else(|| commit.commit.committer.as_ref().and_then(|c| c.date))
            .map(|date| date.to_rfc3339());
        let author = commit
            .commit
            .author
            .as_ref()
            .map(|author| author.name.clone());
        CommitSummary {
            message: commit.commit.message,
            sha: commit.sha,
            author,
            committed_at,
        }
    })
}

fn map_run(run: octocrab::models::workflows::Run) -> WorkflowRun {
    WorkflowRun {
        id: run.id.into_inner(),
        workflow_id: run.workflow_id.into_inner(),
        name: run.name,
        status: RunStatus::parse(&run.status),
        conclusion: run.conclusion,
        branch: Some(run.head_branch),
        event: run.event,
        // octocrab's typed `Run` does not carry the triggering actor.
        actor: None,
        created_at: Some(run.created_at.to_rfc3339()),
        html_url: Some(run.html_url.to_string()),
    }
}

fn map_status(status: &octocrab::models::workflows::Status) -> RunStatus {
    use octocrab::models::workflows::Status;
    match status {
        Status::Pending | Status::Queued | Status::Waiting => RunStatus::Queued,
        Status::InProgress => RunStatus::InProgress,
        Status::Completed => RunStatus::Completed,
        _ => RunStatus::Unknown,
    }
}

fn map_conclusion(conclusion: &Option<octocrab::models::workflows::Conclusion>) -> Option<String> {
    use octocrab::models::workflows::Conclusion;
    conclusion.as_ref().map(|conclusion| {
        match conclusion {
            Conclusion::ActionRequired => "action_required",
            Conclusion::Cancelled => "cancelled",
            Conclusion::Failure => "failure",
            Conclusion::Neutral => "neutral",
            Conclusion::Skipped => "skipped",
            Conclusion::Success => "success",
            Conclusion::TimedOut => "timed_out",
            _ => "unknown",
        }
        .to_owned()
    })
}

fn map_job(job: octocrab::models::workflows::Job) -> Job {
    Job {
        id: job.id.into_inner(),
        name: job.name,
        status: map_status(&job.status),
        conclusion: map_conclusion(&job.conclusion),
        steps: job
            .steps
            .into_iter()
            .map(|step| Step {
                number: step.number,
                name: step.name,
                status: map_status(&step.status),
                conclusion: map_conclusion(&step.conclusion),
            })
            .collect(),
    }
}

#[derive(Serialize)]
struct PollBody<'a> {
    client_id: &'a str,
    device_code: &'a str,
    grant_type: &'a str,
}

#[derive(Deserialize)]
struct PollResponse {
    access_token: Option<String>,
    error: Option<String>,
}

fn unexpected(error: impl std::fmt::Display) -> GatewayError {
    GatewayError::Unexpected(error.to_string())
}

fn map_error(error: octocrab::Error) -> GatewayError {
    let mapped = classify(&error);
    tracing::warn!(kind = %mapped, "GitHub request failed");
    mapped
}

fn classify(error: &octocrab::Error) -> GatewayError {
    match error {
        octocrab::Error::GitHub { source, .. } => match source.status_code.as_u16() {
            401 => GatewayError::Unauthorized,
            403 => GatewayError::RateLimited,
            404 => GatewayError::NotFound,
            _ => GatewayError::Unexpected(source.message.clone()),
        },
        octocrab::Error::Http { .. } | octocrab::Error::Service { .. } => {
            GatewayError::Transport(error.to_string())
        }
        _ => GatewayError::Transport(error.to_string()),
    }
}
