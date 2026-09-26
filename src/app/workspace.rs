use std::collections::HashMap;

use tracing::{debug, info, warn};

use crate::github::{
    FileWrite, GatewayError, GitHubGateway, RunFilter, RunStatusFilter, SecretToken, Workflow,
    WorkflowRun, any_running, split_full_name,
};
use crate::workflow_draft::{DraftProblem, WorkflowDraft, can_run_manually};

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

/// Why the console could not trigger the selected workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunProblem {
    /// No workflow is selected, so there is nothing to run.
    NoWorkflow,
    /// The repository's default branch is unknown, so there is no ref to run on.
    NoDefaultBranch,
    /// GitHub refused the dispatch.
    Gateway(AppProblem),
}

/// 取消或删除一次运行为什么没成。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunActionProblem {
    /// 没有打开的仓库，所以不知道这条运行在哪。
    NoRepository,
    /// GitHub 拒绝了这一下（没权限、运行已经结束、限流……）。
    Gateway(AppProblem),
}

/// 对一次运行做的两件事。取消与删除都落在运行上——GitHub 没有 job 级的这两个操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunAction {
    Cancel,
    Delete,
}

/// Why the console could not save the open workflow file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveProblem {
    /// No workflow is selected, or its file was never read.
    NoWorkflow,
    /// The repository's default branch is unknown, so there is no branch to
    /// commit to.
    NoDefaultBranch,
    /// GitHub refused the write.
    Gateway(AppProblem),
}

/// Why the console could not create a new workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateProblem {
    /// The form does not describe a workflow yet.
    Draft(DraftProblem),
    /// No repository is open.
    NoRepository,
    /// The repository's default branch is unknown, so there is no branch to
    /// commit to.
    NoDefaultBranch,
    /// GitHub refused the write.
    Gateway(AppProblem),
}

/// What a push did to the repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    /// The file was not there before.
    Created,
    /// A file of the same name was replaced.
    Replaced,
}

/// One repository's workspace: its workflows, its runs, and the filters and
/// polling decision that go with them.
pub struct Workspace {
    repository: Option<String>,
    default_branch: Option<String>,
    tab: WorkspaceTab,
    workflows: Vec<Workflow>,
    workflows_state: LoadState,
    selected_workflow: Option<u64>,
    workflow_file: Option<String>,
    workflow_file_sha: Option<String>,
    workflow_file_state: LoadState,
    runner_labels: Vec<String>,
    runner_labels_state: LoadState,
    runs: Vec<WorkflowRun>,
    runs_state: LoadState,
    runs_filter: RunFilter,
    runs_page: u32,
    runs_has_more: bool,
    /// 每条工作流能不能手动触发。GitHub 的工作流列表不说触发方式，只能读文件；读到的
    /// 结果留着，同一个仓库里不重复读，读不到的下次再来。
    runnable: HashMap<u64, bool>,
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
            default_branch: None,
            tab: WorkspaceTab::Workflows,
            workflows: Vec::new(),
            workflows_state: LoadState::Idle,
            selected_workflow: None,
            workflow_file: None,
            workflow_file_sha: None,
            workflow_file_state: LoadState::Idle,
            runner_labels: Vec::new(),
            runner_labels_state: LoadState::Idle,
            runs: Vec::new(),
            runs_state: LoadState::Idle,
            runs_filter: RunFilter::default(),
            runs_page: 0,
            runs_has_more: false,
            runnable: HashMap::new(),
        }
    }

    pub fn repository(&self) -> Option<&str> {
        self.repository.as_deref()
    }

    pub fn default_branch(&self) -> Option<&str> {
        self.default_branch.as_deref()
    }

    pub fn tab(&self) -> WorkspaceTab {
        self.tab
    }

    pub fn set_tab(&mut self, tab: WorkspaceTab) {
        self.tab = tab;
    }

    /// Enter a repository. Changing repositories resets everything the
    /// previous one was showing.
    pub fn enter(&mut self, full_name: &str, default_branch: Option<String>) {
        self.default_branch = default_branch;
        if self.repository.as_deref() != Some(full_name) {
            self.workflows.clear();
            self.workflows_state = LoadState::Idle;
            self.selected_workflow = None;
            self.workflow_file = None;
            self.workflow_file_sha = None;
            self.workflow_file_state = LoadState::Idle;
            self.runner_labels.clear();
            self.runner_labels_state = LoadState::Idle;
            self.runs.clear();
            self.runs_state = LoadState::Idle;
            self.runs_filter = RunFilter::default();
            self.runs_page = 0;
            self.runs_has_more = false;
            self.runnable.clear();
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

    /// 这条工作流现在能不能手动跑（旁边要不要给运行按钮）。还没读到文件、或者读失败，
    /// 就先当不能——按钮宁可不出现，也不要给一个点了会失败的。
    pub fn can_run(&self, workflow_id: u64) -> bool {
        self.runnable.get(&workflow_id).copied().unwrap_or(false)
    }

    /// 每条工作流的触发方式都读一遍：列表里那条工作流旁边该不该有运行按钮，取决于
    /// 它自己文件里有没有 `workflow_dispatch`。已经读过的不再读。
    pub async fn load_workflow_triggers(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
    ) {
        let Some((owner, repository)) = self.parts() else {
            return;
        };
        // 先照一份名单，读文件的时候不再借着 self 的那些字段。
        let pending: Vec<(u64, String)> = self
            .workflows
            .iter()
            .filter(|workflow| !self.runnable.contains_key(&workflow.id))
            .map(|workflow| (workflow.id, workflow.path.clone()))
            .collect();

        for (id, path) in pending {
            match gateway
                .file_contents(token, &owner, &repository, &path)
                .await
            {
                Ok(contents) => {
                    self.runnable.insert(id, can_run_manually(&contents.text));
                }
                // 读不到就先不下结论：下次进这个仓库再读一遍。
                Err(error) => warn!(%error, %path, "could not read a workflow file"),
            }
        }
    }

    /// The workflow the file viewer is showing, if any.
    pub fn selected_workflow(&self) -> Option<&Workflow> {
        let id = self.selected_workflow?;
        self.workflows.iter().find(|workflow| workflow.id == id)
    }

    pub fn selected_workflow_id(&self) -> Option<u64> {
        self.selected_workflow
    }

    /// The YAML of the selected workflow, once it has been read.
    pub fn workflow_file(&self) -> Option<&str> {
        self.workflow_file.as_deref()
    }

    pub fn workflow_file_state(&self) -> LoadState {
        self.workflow_file_state
    }

    /// 哪些工作流能手动跑，按 id 一份。界面上换一批工作流时整份换掉。
    pub fn runnable(&self) -> &HashMap<u64, bool> {
        &self.runnable
    }

    /// The repository's own runner labels, on top of the hosted ones the
    /// console carries.
    pub fn runner_labels(&self) -> &[String] {
        &self.runner_labels
    }

    pub fn runner_labels_state(&self) -> LoadState {
        self.runner_labels_state
    }

    /// Read the repository's self-hosted runner labels. A repository without
    /// any — or without the rights to list them — keeps the hosted list only.
    pub async fn load_runner_labels(&mut self, gateway: &dyn GitHubGateway, token: &SecretToken) {
        let Some((owner, repository)) = self.parts() else {
            return;
        };

        debug!(repository = %repository, "loading runner labels");
        self.runner_labels_state = LoadState::Loading;
        match gateway.runner_labels(token, &owner, &repository).await {
            Ok(labels) => {
                let mut labels: Vec<String> = labels
                    .into_iter()
                    .map(|label| label.trim().to_owned())
                    .filter(|label| !label.is_empty())
                    .collect();
                labels.sort();
                labels.dedup();
                info!(count = labels.len(), "runner labels loaded");
                self.runner_labels = labels;
                self.runner_labels_state = LoadState::Loaded;
            }
            Err(error) => {
                // Runner labels are a convenience: a repository the user cannot
                // administer simply keeps the hosted list.
                warn!(%error, repository = %repository, "could not load runner labels");
                self.runner_labels.clear();
                self.runner_labels_state = LoadState::failed(&error);
            }
        }
    }

    /// Show another workflow. Its file is fetched by [`Self::load_workflow_file`].
    pub fn select_workflow(&mut self, workflow_id: u64) {
        self.set_selected_workflow(Some(workflow_id));
    }

    fn set_selected_workflow(&mut self, workflow_id: Option<u64>) {
        if self.selected_workflow == workflow_id {
            return;
        }
        self.selected_workflow = workflow_id;
        self.workflow_file = None;
        self.workflow_file_sha = None;
        self.workflow_file_state = LoadState::Idle;
    }

    /// Opening a repository should show something: the first workflow stands in
    /// whenever nothing is selected or the selection is gone.
    fn keep_or_default_selection(&mut self) {
        let still_listed = self
            .selected_workflow
            .is_some_and(|id| self.workflows.iter().any(|workflow| workflow.id == id));
        if !still_listed {
            let first = self.workflows.first().map(|workflow| workflow.id);
            self.set_selected_workflow(first);
        }
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

        debug!(repository = %repository, "loading workflows");
        self.workflows_state = LoadState::Loading;
        match gateway.list_workflows(token, &owner, &repository).await {
            Ok(workflows) => {
                info!(count = workflows.len(), "workflows loaded");
                self.workflows = workflows;
                self.workflows_state = LoadState::Loaded;
                self.keep_or_default_selection();
            }
            Err(error) => {
                warn!(%error, repository = %repository, "could not load workflows");
                self.workflows_state = LoadState::failed(&error);
            }
        }

        self.load_workflow_file(gateway, token).await;
    }

    /// Read the YAML of the selected workflow. Without a selection, or without
    /// an open repository, there is nothing to read.
    pub async fn load_workflow_file(&mut self, gateway: &dyn GitHubGateway, token: &SecretToken) {
        let Some((owner, repository)) = self.parts() else {
            return;
        };
        let Some(workflow) = self.selected_workflow().cloned() else {
            return;
        };

        debug!(path = %workflow.path, "loading a workflow file");
        self.workflow_file_state = LoadState::Loading;
        match gateway
            .file_contents(token, &owner, &repository, &workflow.path)
            .await
        {
            Ok(contents) => {
                self.workflow_file = Some(contents.text);
                self.workflow_file_sha = Some(contents.sha);
                self.workflow_file_state = LoadState::Loaded;
            }
            Err(error) => {
                warn!(%error, path = %workflow.path, "could not load a workflow file");
                self.workflow_file = None;
                self.workflow_file_sha = None;
                self.workflow_file_state = LoadState::failed(&error);
            }
        }
    }

    /// Commit `contents` as the new text of the selected workflow's file, on the
    /// repository's default branch. The file is re-read afterwards so the
    /// console and the repository agree on the new revision.
    pub async fn save_workflow_file(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        contents: &str,
    ) -> Result<(), SaveProblem> {
        let Some((owner, repository)) = self.parts() else {
            return Err(SaveProblem::NoWorkflow);
        };
        let Some(workflow) = self.selected_workflow().cloned() else {
            return Err(SaveProblem::NoWorkflow);
        };
        let Some(sha) = self.workflow_file_sha.clone() else {
            return Err(SaveProblem::NoWorkflow);
        };
        let Some(reference) = self.default_branch.clone() else {
            return Err(SaveProblem::NoDefaultBranch);
        };

        info!(path = %workflow.path, %reference, "saving a workflow file");
        let write = FileWrite {
            message: format!("chore: 更新 {}", workflow.path),
            path: workflow.path.clone(),
            contents: contents.to_owned(),
            reference,
            sha: Some(sha),
        };
        gateway
            .write_file(token, &owner, &repository, write)
            .await
            .map_err(|error| SaveProblem::Gateway(AppProblem::from_gateway(&error)))?;

        // 文件换了内容，触发方式可能也换了：下次再读一遍这份。
        self.runnable.remove(&workflow.id);
        self.load_workflow_file(gateway, token).await;
        Ok(())
    }

    /// Write the workflow file, then show it. A file already at that path is
    /// replaced, which is what the push button promises.
    pub async fn create_workflow(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        draft: &WorkflowDraft,
    ) -> Result<PushOutcome, CreateProblem> {
        let yaml = draft.to_yaml().map_err(CreateProblem::Draft)?;
        let path = draft.path().map_err(CreateProblem::Draft)?;
        self.push_workflow_file(gateway, token, &path, &yaml, "由表单生成")
            .await
            .map(|(outcome, _)| outcome)
    }

    /// 把一段现成的 workflow 文本推到仓库的默认分支：有就覆盖，没有就新建，然后把它
    /// 显示出来。发布模板走的就是这条路——文本不由表单生成，而是直接采用的那一份。
    pub async fn push_workflow_file(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        path: &str,
        contents: &str,
        what: &str,
    ) -> Result<(PushOutcome, String), CreateProblem> {
        let Some((owner, repository)) = self.parts() else {
            return Err(CreateProblem::NoRepository);
        };
        let Some(reference) = self.default_branch.clone() else {
            return Err(CreateProblem::NoDefaultBranch);
        };

        // The read decides both questions at once: whether the file is already
        // there, and which revision a replacement has to start from.
        let (outcome, sha) = match gateway
            .file_contents(token, &owner, &repository, path)
            .await
        {
            Ok(existing) => (PushOutcome::Replaced, Some(existing.sha)),
            Err(GatewayError::NotFound) => (PushOutcome::Created, None),
            Err(error) => {
                return Err(CreateProblem::Gateway(AppProblem::from_gateway(&error)));
            }
        };

        info!(%path, %reference, ?outcome, "pushing a workflow file");
        let write = FileWrite {
            message: match outcome {
                PushOutcome::Created => format!("chore: 添加工作流 {path}（{what}）"),
                PushOutcome::Replaced => format!("chore: 更新工作流 {path}（{what}）"),
            },
            path: path.to_owned(),
            contents: contents.to_owned(),
            reference,
            sha,
        };
        gateway
            .write_file(token, &owner, &repository, write)
            .await
            .map_err(|error| CreateProblem::Gateway(AppProblem::from_gateway(&error)))?;

        self.load_workflows(gateway, token).await;
        self.show_workflow_at(path, gateway, token).await;
        Ok((outcome, path.to_owned()))
    }

    /// Select the workflow a path belongs to, if the repository has one. A file
    /// the reload already put on screen is not read again.
    async fn show_workflow_at(
        &mut self,
        path: &str,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
    ) {
        let Some(workflow) = self
            .workflows
            .iter()
            .find(|workflow| workflow.path == path)
            .cloned()
        else {
            return;
        };
        if self.selected_workflow == Some(workflow.id) && self.workflow_file.is_some() {
            return;
        }

        self.select_workflow(workflow.id);
        self.load_workflow_file(gateway, token).await;
    }

    /// Trigger the selected workflow on the repository's default branch.
    pub async fn run_selected_workflow(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
    ) -> Result<(), RunProblem> {
        let Some(workflow_id) = self.selected_workflow else {
            return Err(RunProblem::NoWorkflow);
        };
        self.run_workflow(gateway, token, workflow_id).await
    }

    /// 跑指定的那条工作流——列表里每条工作流旁边的运行按钮点下去的就是这一条，
    /// 不必先把它选中。
    pub async fn run_workflow(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        workflow_id: u64,
    ) -> Result<(), RunProblem> {
        let Some((owner, repository)) = self.parts() else {
            return Err(RunProblem::NoWorkflow);
        };
        let Some(reference) = self.default_branch.clone() else {
            return Err(RunProblem::NoDefaultBranch);
        };

        info!(workflow_id, %reference, "dispatching a workflow run");
        gateway
            .dispatch_workflow(
                token,
                &owner,
                &repository,
                &workflow_id.to_string(),
                &reference,
                &[],
            )
            .await
            .map_err(|error| RunProblem::Gateway(AppProblem::from_gateway(&error)))
    }

    /// 取消一次还没跑完的运行：GitHub 的取消只有运行这一级，进行中的 job 跟着停。
    /// 取消完把列表重新读一遍，让状态是刚问 GitHub 要来的那一个。
    pub async fn cancel_run(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        run_id: u64,
    ) -> Result<(), RunActionProblem> {
        self.run_action(gateway, token, run_id, RunAction::Cancel)
            .await
    }

    /// 删掉一次已经跑完的运行：连同它的日志与构建产物。GitHub 没有"删单个 job"，
    /// 能删的就是运行本身。
    pub async fn delete_run(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        run_id: u64,
    ) -> Result<(), RunActionProblem> {
        self.run_action(gateway, token, run_id, RunAction::Delete)
            .await
    }

    async fn run_action(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        run_id: u64,
        action: RunAction,
    ) -> Result<(), RunActionProblem> {
        let Some((owner, repository)) = self.parts() else {
            return Err(RunActionProblem::NoRepository);
        };

        info!(run_id, ?action, "changing a run");
        let outcome = match action {
            RunAction::Cancel => {
                gateway
                    .cancel_workflow_run(token, &owner, &repository, run_id)
                    .await
            }
            RunAction::Delete => {
                gateway
                    .delete_workflow_run(token, &owner, &repository, run_id)
                    .await
            }
        };
        outcome.map_err(|error| RunActionProblem::Gateway(AppProblem::from_gateway(&error)))?;

        self.reload_runs(gateway, token).await;
        Ok(())
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
                info!(
                    count = page.runs.len(),
                    running = crate::github::any_running(&page.runs),
                    "runs loaded"
                );
                self.runs = page.runs;
                self.runs_has_more = page.has_more;
                self.runs_page = 1;
                self.runs_state = LoadState::Loaded;
            }
            Err(error) => {
                warn!(%error, "could not load runs");
                self.runs_state = LoadState::failed(&error);
            }
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
                debug!(count = page.runs.len(), page = next, "more runs loaded");
                self.runs.extend(page.runs);
                self.runs_has_more = page.has_more;
                self.runs_page = next;
                self.runs_state = LoadState::Loaded;
            }
            Err(error) => {
                warn!(%error, page = next, "could not load more runs");
                self.runs_state = LoadState::failed(&error);
            }
        }
    }
}
