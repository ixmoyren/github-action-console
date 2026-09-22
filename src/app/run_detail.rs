use crate::github::{GatewayError, GitHubGateway, Job, SecretToken, WorkflowRun, split_full_name};

use super::workspace::LoadState;

/// One run's detail: its jobs, their steps, and the log of the selected job.
pub struct RunDetail {
    repository: Option<String>,
    run_id: Option<u64>,
    html_url: Option<String>,
    jobs: Vec<Job>,
    state: LoadState,
    selected_job: Option<u64>,
    logs: Option<String>,
    logs_state: LoadState,
}

impl Default for RunDetail {
    fn default() -> Self {
        Self::new()
    }
}

impl RunDetail {
    pub fn new() -> Self {
        Self {
            repository: None,
            run_id: None,
            html_url: None,
            jobs: Vec::new(),
            state: LoadState::Idle,
            selected_job: None,
            logs: None,
            logs_state: LoadState::Idle,
        }
    }

    /// Open a run inside a repository.
    pub fn open(&mut self, repository: &str, run: &WorkflowRun) {
        self.repository = Some(repository.to_owned());
        self.run_id = Some(run.id);
        self.html_url = run.html_url.clone();
        self.jobs.clear();
        self.state = LoadState::Idle;
        self.selected_job = None;
        self.logs = None;
        self.logs_state = LoadState::Idle;
    }

    pub fn close(&mut self) {
        *self = Self::new();
    }

    pub fn repository(&self) -> Option<&str> {
        self.repository.as_deref()
    }

    pub fn run_id(&self) -> Option<u64> {
        self.run_id
    }

    /// The run's GitHub page, for "open in browser".
    pub fn html_url(&self) -> Option<&str> {
        self.html_url.as_deref()
    }

    pub fn jobs(&self) -> &[Job] {
        &self.jobs
    }

    pub fn state(&self) -> LoadState {
        self.state
    }

    pub fn selected_job(&self) -> Option<u64> {
        self.selected_job
    }

    pub fn logs(&self) -> Option<&str> {
        self.logs.as_deref()
    }

    pub fn logs_state(&self) -> LoadState {
        self.logs_state
    }

    /// Pick which job's log to look at. Discards the previous log.
    pub fn select_job(&mut self, job_id: u64) {
        if self.selected_job != Some(job_id) {
            self.selected_job = Some(job_id);
            self.logs = None;
            self.logs_state = LoadState::Idle;
        }
    }

    fn parts(&self) -> Option<(String, String)> {
        self.repository.as_deref().and_then(split_full_name)
    }

    pub async fn load_jobs(&mut self, gateway: &dyn GitHubGateway, token: &SecretToken) {
        let (Some((owner, repository)), Some(run_id)) = (self.parts(), self.run_id) else {
            return;
        };

        self.state = LoadState::Loading;
        match gateway.list_jobs(token, &owner, &repository, run_id).await {
            Ok(jobs) => {
                self.jobs = jobs;
                self.state = LoadState::Loaded;
            }
            Err(error) => self.state = failed(&error),
        }
    }

    pub async fn load_logs(&mut self, gateway: &dyn GitHubGateway, token: &SecretToken) {
        let (Some((owner, repository)), Some(job_id)) = (self.parts(), self.selected_job) else {
            return;
        };

        self.logs_state = LoadState::Loading;
        match gateway.job_logs(token, &owner, &repository, job_id).await {
            Ok(logs) => {
                self.logs = Some(logs);
                self.logs_state = LoadState::Loaded;
            }
            Err(error) => self.logs_state = failed(&error),
        }
    }
}

fn failed(error: &GatewayError) -> LoadState {
    LoadState::Failed(super::repositories::AppProblem::from_gateway(error))
}
