use crate::github::{
    GatewayError, GitHubGateway, RunFilter, RunStatusFilter, SecretToken, Workflow, WorkflowRun,
    any_running, split_full_name,
};

use super::repositories::AppProblem;

const PAGE_SIZE: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkspaceTab {
    #[default]
    Workflows,
    Runs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadState {
    Idle,
    Loading,
    Loaded,
    Failed(AppProblem),
}

impl LoadState {
    fn failed(error: &GatewayError) -> Self {
        Self::Failed(AppProblem::from_gateway(error))
    }
}

/// One repository's workspace: its workflows, its runs, and the filters and
/// polling decision that go with them.
pub struct Workspace {
    repository: Option<String>,
    tab: WorkspaceTab,
    workflows: Vec<Workflow>,
    workflows_state: LoadState,
    runs: Vec<WorkflowRun>,
    runs_state: LoadState,
    runs_filter: RunFilter,
    runs_page: u32,
    runs_has_more: bool,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    pub fn new() -> Self {
        Self {
            repository: None,
            tab: WorkspaceTab::Workflows,
            workflows: Vec::new(),
            workflows_state: LoadState::Idle,
            runs: Vec::new(),
            runs_state: LoadState::Idle,
            runs_filter: RunFilter::default(),
            runs_page: 0,
            runs_has_more: false,
        }
    }

    pub fn repository(&self) -> Option<&str> {
        self.repository.as_deref()
    }

    pub fn tab(&self) -> WorkspaceTab {
        self.tab
    }

    pub fn set_tab(&mut self, tab: WorkspaceTab) {
        self.tab = tab;
    }

    /// Enter a repository. Changing repositories resets everything the
    /// previous one was showing.
    pub fn enter(&mut self, full_name: &str) {
        if self.repository.as_deref() != Some(full_name) {
            self.workflows.clear();
            self.workflows_state = LoadState::Idle;
            self.runs.clear();
            self.runs_state = LoadState::Idle;
            self.runs_filter = RunFilter::default();
            self.runs_page = 0;
            self.runs_has_more = false;
            self.tab = WorkspaceTab::Workflows;
            self.repository = Some(full_name.to_owned());
        }
    }

    /// Leave the repository. Stops any polling that was running.
    pub fn leave(&mut self) {
        *self = Self::new();
    }

    pub fn workflows(&self) -> &[Workflow] {
        &self.workflows
    }

    pub fn workflows_state(&self) -> LoadState {
        self.workflows_state
    }

    pub fn runs(&self) -> &[WorkflowRun] {
        &self.runs
    }

    pub fn runs_state(&self) -> LoadState {
        self.runs_state
    }

    pub fn runs_has_more(&self) -> bool {
        self.runs_has_more
    }

    pub fn run_filter(&self) -> &RunFilter {
        &self.runs_filter
    }

    pub fn set_status_filter(&mut self, status: RunStatusFilter) {
        self.runs_filter.status = status;
    }

    pub fn set_branch_filter(&mut self, branch: Option<String>) {
        self.runs_filter.branch = branch.filter(|value| !value.trim().is_empty());
    }

    /// Setting a workflow filter changes which endpoint the next fetch uses.
    pub fn set_workflow_filter(&mut self, workflow_id: Option<u64>) {
        self.runs_filter.workflow_id = workflow_id;
    }

    pub fn visible_runs(&self) -> Vec<WorkflowRun> {
        crate::github::filter_runs(&self.runs, &self.runs_filter)
    }

    pub fn has_running_runs(&self) -> bool {
        any_running(&self.runs)
    }

    /// The console polls the runs of the open repository only while the runs
    /// tab is in front and something is still moving. Switching tabs or
    /// leaving the repository therefore stops polling.
    pub fn should_poll_runs(&self) -> bool {
        self.repository.is_some() && self.tab == WorkspaceTab::Runs && self.has_running_runs()
    }

    fn parts(&self) -> Option<(String, String)> {
        self.repository.as_deref().and_then(split_full_name)
    }

    pub async fn load_workflows(&mut self, gateway: &dyn GitHubGateway, token: &SecretToken) {
        let Some((owner, repository)) = self.parts() else {
            return;
        };

        self.workflows_state = LoadState::Loading;
        match gateway.list_workflows(token, &owner, &repository).await {
            Ok(workflows) => {
                self.workflows = workflows;
                self.workflows_state = LoadState::Loaded;
            }
            Err(error) => self.workflows_state = LoadState::failed(&error),
        }
    }

    pub async fn reload_runs(&mut self, gateway: &dyn GitHubGateway, token: &SecretToken) {
        let Some((owner, repository)) = self.parts() else {
            return;
        };

        self.runs_state = LoadState::Loading;
        match gateway
            .list_workflow_runs(
                token,
                &owner,
                &repository,
                self.runs_filter.workflow_id,
                1,
                PAGE_SIZE,
            )
            .await
        {
            Ok(page) => {
                self.runs = page.runs;
                self.runs_has_more = page.has_more;
                self.runs_page = 1;
                self.runs_state = LoadState::Loaded;
            }
            Err(error) => self.runs_state = LoadState::failed(&error),
        }
    }

    pub async fn load_more_runs(&mut self, gateway: &dyn GitHubGateway, token: &SecretToken) {
        if !self.runs_has_more || self.runs_state == LoadState::Loading {
            return;
        }
        let Some((owner, repository)) = self.parts() else {
            return;
        };

        let next = self.runs_page + 1;
        self.runs_state = LoadState::Loading;
        match gateway
            .list_workflow_runs(
                token,
                &owner,
                &repository,
                self.runs_filter.workflow_id,
                next,
                PAGE_SIZE,
            )
            .await
        {
            Ok(page) => {
                self.runs.extend(page.runs);
                self.runs_has_more = page.has_more;
                self.runs_page = next;
                self.runs_state = LoadState::Loaded;
            }
            Err(error) => self.runs_state = LoadState::failed(&error),
        }
    }
}
