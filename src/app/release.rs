//! The release board: a repository's manifest, its channel pointers, and the
//! builds the console asked for.
//!
//! Definitions come from the repository's manifest (ADR-0003); pointers and
//! dispatch records live in SQLite. A build turning green never moves a pointer
//! — only the explicit publish action does (ADR-0005).

use tracing::{debug, info, warn};

use crate::github::{
    CommitSummary, FileWrite, GatewayError, GitHubGateway, Job, PullRequest, ReleaseAsset,
    SecretToken, WorkflowRun, split_full_name,
};
use crate::release::{
    BuildDispatch, CHANNELS, ChannelPointer, MANIFEST_PATH, ManifestProblem, ReleaseManifest,
    ReleaseState, ReleaseTarget, parse_manifest, release_state_for_job,
};
use crate::store::Store;

use super::repositories::AppProblem;

use super::LoadState;

/// How far the manifest read has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ManifestState {
    #[default]
    Idle,
    Loading,
    Loaded,
    /// The repository has no manifest yet: a state of its own, not a failure.
    Missing,
    /// The manifest is there but the console cannot read it.
    Invalid,
    Failed(AppProblem),
}

/// Why a build could not be triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerProblem {
    NoRepository,
    /// No readable manifest, so no recipe to dispatch.
    NoManifest,
    /// The manifest does not define that target or its packaging config.
    UnknownTarget,
    /// The console could not record what it asked for.
    Store,
    Gateway(AppProblem),
}

/// Why a release could not be published to a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishProblem {
    NoRepository,
    NoManifest,
    UnknownTarget,
    /// The version carries no asset for this target yet: nothing to publish.
    NothingToPublish,
    /// The console could not record the new pointer.
    Store,
    Gateway(AppProblem),
}

/// Why a channel tag could not be created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagProblem {
    NoRepository,
    /// 分支上读不到提交，也就没得可指。
    NoCommit,
    Gateway(AppProblem),
}

/// 一个通道 tag 这一次运行取回来的 job。
///
/// 看板格子里的状态来自它：tag 推上去触发的那个 run，按 job 名认领各个发布目标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagJobs {
    /// 取的是哪个 run 的 job——run 换了就要重取。
    pub run_id: u64,
    pub state: LoadState,
    pub jobs: Vec<Job>,
}

/// Why a manifest could not be sent as a pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestWriteProblem {
    NoRepository,
    /// The repository's default branch is unknown, so there is no base to open
    /// the pull request against.
    NoBase,
    /// The manifest is not readable, so there is nothing to write back.
    Gateway(AppProblem),
}

/// The pull request a manifest write turned into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestWrite {
    pub branch: String,
    pub pull_number: u64,
}

/// What a run produced, as three separate facts: it finished, what is
/// downloadable, and whether a channel points at it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseFacts {
    /// The version the console asked to build.
    pub version: String,
    pub target: String,
    pub config: String,
    /// 产物可获取: the release assets for this version and target.
    pub assets: Vec<ReleaseAsset>,
    /// 已登记发布: the channels pointing at this version for this target.
    pub pointers: Vec<ChannelPointer>,
    /// Why the assets could not be read, when they could not.
    pub problem: Option<AppProblem>,
}

/// A repository's releases, as far as the console knows them.
pub struct ReleaseBoard {
    store: Store,
    repository: Option<String>,
    manifest: Option<ReleaseManifest>,
    /// The manifest as it was read, for the editor to seed from.
    manifest_text: Option<String>,
    manifest_state: ManifestState,
    manifest_problem: Option<ManifestProblem>,
    pointers: Vec<ChannelPointer>,
    dispatches: Vec<BuildDispatch>,
    /// 每个通道 tag 那次运行取回来的 job，按 tag 名存。
    tag_jobs: std::collections::BTreeMap<String, TagJobs>,
}

impl ReleaseBoard {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            repository: None,
            manifest: None,
            manifest_text: None,
            manifest_state: ManifestState::Idle,
            manifest_problem: None,
            pointers: Vec::new(),
            dispatches: Vec::new(),
            tag_jobs: std::collections::BTreeMap::new(),
        }
    }

    pub fn repository(&self) -> Option<&str> {
        self.repository.as_deref()
    }

    /// Enter a repository. Another repository means starting over.
    pub fn enter(&mut self, repository: &str) {
        if self.repository.as_deref() != Some(repository) {
            self.manifest = None;
            self.manifest_text = None;
            self.manifest_state = ManifestState::Idle;
            self.manifest_problem = None;
            self.pointers.clear();
            self.dispatches.clear();
            self.tag_jobs.clear();
            self.repository = Some(repository.to_owned());
        }
    }

    pub fn leave(&mut self) {
        *self = Self::new(self.store.clone());
    }

    pub fn manifest(&self) -> Option<&ReleaseManifest> {
        self.manifest.as_ref()
    }

    /// The manifest exactly as the repository has it.
    pub fn manifest_text(&self) -> Option<&str> {
        self.manifest_text.as_deref()
    }

    pub fn manifest_state(&self) -> ManifestState {
        self.manifest_state
    }

    pub fn manifest_problem(&self) -> Option<&ManifestProblem> {
        self.manifest_problem.as_ref()
    }

    pub fn pointers(&self) -> &[ChannelPointer] {
        &self.pointers
    }

    pub fn dispatches(&self) -> &[BuildDispatch] {
        &self.dispatches
    }

    pub fn pointer(&self, target: &str, channel: &str) -> Option<&ChannelPointer> {
        self.pointers
            .iter()
            .find(|pointer| pointer.target == target && pointer.channel == channel)
    }

    /// Read the manifest and the pointers. Both halves are independent: a
    /// missing manifest still lets the board show the pointers it has.
    pub async fn load(&mut self, gateway: &dyn GitHubGateway, token: &SecretToken) {
        self.load_manifest(gateway, token).await;
        self.load_pointers().await;
    }

    pub async fn load_manifest(&mut self, gateway: &dyn GitHubGateway, token: &SecretToken) {
        let Some((owner, repository)) = self.parts() else {
            return;
        };

        debug!(repository = %repository, "loading the release manifest");
        self.manifest_state = ManifestState::Loading;
        self.manifest_problem = None;
        match gateway
            .file_contents(token, &owner, &repository, MANIFEST_PATH)
            .await
        {
            Ok(contents) => match parse_manifest(&contents.text) {
                Ok(manifest) => {
                    self.manifest_text = Some(contents.text.clone());
                    info!(
                        targets = manifest.targets().len(),
                        packaging = manifest.packaging().len(),
                        "release manifest loaded"
                    );
                    self.manifest = Some(manifest);
                    self.manifest_state = ManifestState::Loaded;
                }
                Err(problem) => {
                    warn!(problem = %problem, "the release manifest cannot be read");
                    self.manifest = None;
                    self.manifest_text = Some(contents.text.clone());
                    self.manifest_problem = Some(problem);
                    self.manifest_state = ManifestState::Invalid;
                }
            },
            Err(GatewayError::NotFound) => {
                debug!(repository = %repository, "the repository has no release manifest");
                self.manifest = None;
                self.manifest_text = None;
                self.manifest_state = ManifestState::Missing;
            }
            Err(error) => {
                warn!(%error, repository = %repository, "could not read the release manifest");
                self.manifest = None;
                self.manifest_text = None;
                self.manifest_state = ManifestState::Failed(AppProblem::from_gateway(&error));
            }
        }
    }

    pub async fn load_pointers(&mut self) {
        let Some(repository) = self.repository.clone() else {
            return;
        };

        match self.store.load_channel_pointers(&repository).await {
            Ok(pointers) => self.pointers = pointers,
            Err(error) => {
                warn!(%error, repository = %repository, "could not read the channel pointers");
                self.pointers.clear();
            }
        }
        match self.store.load_dispatches(&repository).await {
            Ok(dispatches) => self.dispatches = dispatches,
            Err(error) => {
                warn!(%error, repository = %repository, "could not read the dispatch log");
                self.dispatches.clear();
            }
        }
    }

    /// Trigger a build for one target, and record what was asked for.
    pub async fn trigger(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        target: &str,
        version: &str,
        config: Option<&str>,
    ) -> Result<BuildDispatch, TriggerProblem> {
        let Some((owner, repository)) = self.parts() else {
            return Err(TriggerProblem::NoRepository);
        };
        let Some(full_name) = self.repository.clone() else {
            return Err(TriggerProblem::NoRepository);
        };
        let Some(manifest) = self.manifest.clone() else {
            return Err(TriggerProblem::NoManifest);
        };
        let Some((target, declared)) = manifest.recipe_for(target) else {
            return Err(TriggerProblem::UnknownTarget);
        };
        // 打包配置可以被显式覆盖（例如临时换一份配方），但覆盖的名字必须也在
        // 清单里，否则这就是一次"清单没描述过的构建"。
        let config_name = match config.map(str::trim).filter(|name| !name.is_empty()) {
            Some(requested) => requested.to_owned(),
            None => declared.name.clone(),
        };
        let Some(config) = manifest.packaging_config(&config_name) else {
            return Err(TriggerProblem::UnknownTarget);
        };
        let workflow = config.workflow.clone();
        let target_name = target.name.clone();
        // 控制台发的是"清单里写的那份配方"：目标、版本，加上配置自带的 inputs。
        let mut inputs = vec![
            ("target".to_owned(), target_name.clone()),
            ("version".to_owned(), version.to_owned()),
            ("config".to_owned(), config_name.clone()),
        ];
        inputs.extend(
            config
                .inputs
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );

        info!(target = %target_name, %version, %workflow, "triggering a build");
        gateway
            .dispatch_workflow(token, &owner, &repository, &workflow, version, &inputs)
            .await
            .map_err(|error| TriggerProblem::Gateway(AppProblem::from_gateway(&error)))?;

        let dispatch = BuildDispatch {
            id: 0,
            // The dispatch and pointer records are keyed by the repository the
            // console is open on, not by the half an API call uses.
            repository: full_name,
            target: target_name,
            version: version.to_owned(),
            config: config_name,
            dispatched_at: chrono::Utc::now().to_rfc3339(),
            run_id: None,
        };
        match self.store.record_dispatch(&dispatch).await {
            Ok(id) => {
                let dispatch = BuildDispatch { id, ..dispatch };
                self.dispatches.push(dispatch.clone());
                Ok(dispatch)
            }
            Err(error) => {
                warn!(%error, "could not record the dispatch");
                Err(TriggerProblem::Store)
            }
        }
    }

    /// Bind dispatches to the runs they turned into, by taking the newest
    /// dispatch-triggered run that started after it and is not claimed yet.
    pub async fn bind_dispatches(&mut self, runs: &[WorkflowRun]) {
        let mut bound = Vec::new();
        // 先算出已经被认领的 run，再改 dispatch，免得同时借用两次。
        let mut claimed = self
            .dispatches
            .iter()
            .filter_map(|dispatch| dispatch.run_id)
            .collect::<Vec<_>>();
        for dispatch in self.dispatches.iter_mut().filter(|d| d.run_id.is_none()) {
            let Some(run) = runs
                .iter()
                .filter(|run| run.event == "workflow_dispatch")
                .filter(|run| !claimed.contains(&run.id))
                .filter(|run| {
                    run.created_at
                        .as_deref()
                        .is_some_and(|created| created >= dispatch.dispatched_at.as_str())
                })
                .min_by(|left, right| left.created_at.cmp(&right.created_at))
            else {
                continue;
            };

            dispatch.run_id = Some(run.id);
            claimed.push(run.id);
            bound.push((dispatch.id, run.id));
        }

        for (id, run_id) in bound {
            if let Err(error) = self.store.bind_dispatch(id, run_id).await {
                warn!(%error, "could not bind a dispatch to its run");
            }
        }
    }

    /// Point one channel at one version for one target. Refused when the version
    /// carries no asset for that target: a green build is not a release.
    pub async fn publish(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        target: &str,
        channel: &str,
        version: &str,
    ) -> Result<ChannelPointer, PublishProblem> {
        let Some((owner, repository)) = self.parts() else {
            return Err(PublishProblem::NoRepository);
        };
        let Some(full_name) = self.repository.clone() else {
            return Err(PublishProblem::NoRepository);
        };
        let Some(manifest) = self.manifest.as_ref() else {
            return Err(PublishProblem::NoManifest);
        };
        if manifest.target(target).is_none() {
            return Err(PublishProblem::UnknownTarget);
        }

        let assets = match gateway
            .release_assets(token, &owner, &repository, version)
            .await
        {
            Ok(assets) => assets,
            Err(GatewayError::NotFound) => Vec::new(),
            Err(error) => {
                return Err(PublishProblem::Gateway(AppProblem::from_gateway(&error)));
            }
        };
        if !version_covers_target(&assets, target) {
            warn!(%target, %version, "that version carries no asset for this target");
            return Err(PublishProblem::NothingToPublish);
        }

        let pointer = ChannelPointer {
            repository: full_name,
            target: target.to_owned(),
            channel: channel.to_owned(),
            version: version.to_owned(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        info!(%target, %channel, %version, "publishing a release to a channel");
        if let Err(error) = self.store.save_channel_pointer(&pointer).await {
            warn!(%error, "could not save the channel pointer");
            return Err(PublishProblem::Store);
        }

        self.pointers
            .retain(|existing| !(existing.target == target && existing.channel == channel));
        self.pointers.push(pointer.clone());
        self.pointers.sort_by(|left, right| {
            (&left.target, &left.channel).cmp(&(&right.target, &right.channel))
        });
        Ok(pointer)
    }

    /// Where one cell of the board stands: the pointer decides "已发布", the
    /// newest dispatch (and the run it became) decides everything before that.
    pub fn cell(&self, target: &ReleaseTarget, channel: &str, runs: &[WorkflowRun]) -> ReleaseCell {
        let pointer = self.pointer(&target.name, channel);
        let run_id = BuildDispatch::latest_for(&self.dispatches, &target.name)
            .and_then(|dispatch| dispatch.run_id);
        let latest = run_id.and_then(|id| runs.iter().find(|run| run.id == id));
        ReleaseCell {
            pointer: pointer.cloned(),
            state: crate::release::release_state(target, pointer.is_some(), latest),
            run_id,
        }
    }

    /// 把通道 tag 指到 `sha` 上；tag 已经存在就移过去。
    ///
    /// 这一步就是"推送到 repo"：ref 在 GitHub 那边建好，推 tag 触发的发布工作流随
    /// 即开始跑。
    pub async fn create_tag(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        tag: &str,
        sha: &str,
    ) -> Result<(), TagProblem> {
        let Some((owner, repository)) = self.parts() else {
            return Err(TagProblem::NoRepository);
        };
        info!(%tag, %sha, "pointing a channel tag at a commit");
        gateway
            .create_tag(token, &owner, &repository, tag, sha)
            .await
            .map_err(|error| TagProblem::Gateway(AppProblem::from_gateway(&error)))
    }

    /// 一条分支上最近的提交：界面拿它填「提交」下拉。
    pub async fn branch_commits(
        &self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        branch: &str,
        limit: u8,
    ) -> Result<Vec<CommitSummary>, TagProblem> {
        let Some((owner, repository)) = self.parts() else {
            return Err(TagProblem::NoRepository);
        };
        gateway
            .branch_commits(token, &owner, &repository, branch, limit)
            .await
            .map_err(|error| TagProblem::Gateway(AppProblem::from_gateway(&error)))
    }

    /// 仓库现在有哪些分支：界面拿它填「分支」下拉。
    pub async fn branches(
        &self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
    ) -> Result<Vec<String>, TagProblem> {
        let Some((owner, repository)) = self.parts() else {
            return Err(TagProblem::NoRepository);
        };
        gateway
            .branches(token, &owner, &repository)
            .await
            .map_err(|error| TagProblem::Gateway(AppProblem::from_gateway(&error)))
    }

    /// 哪些通道 tag 的 job 还没取（或者取的还是上一个 run）。
    ///
    /// 视图拿它去问 GitHub；问回来之后格子才有状态可说。
    pub fn tag_jobs_to_load(&self, runs: &[WorkflowRun]) -> Vec<(String, u64)> {
        let mut wanted = Vec::new();
        for tag in CHANNELS {
            let Some(run) = tag_run(runs, tag) else {
                continue;
            };
            match self.tag_jobs.get(tag) {
                // 这个 run 的 job 已经取过了。
                Some(jobs) if jobs.run_id == run.id => {}
                _ => wanted.push((tag.to_owned(), run.id)),
            }
        }
        wanted
    }

    /// 记下"这个 tag 那次运行的 job 正在取"，免得每一帧都要一遍。
    pub fn begin_tag_jobs(&mut self, tag: &str, run_id: u64) {
        self.tag_jobs.insert(
            tag.to_owned(),
            TagJobs {
                run_id,
                state: LoadState::Loading,
                jobs: Vec::new(),
            },
        );
    }

    /// 取一个通道 tag 那次运行里的 job。
    pub async fn load_tag_jobs(
        &mut self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        tag: &str,
        run_id: u64,
    ) {
        let Some((owner, repository)) = self.parts() else {
            return;
        };
        match gateway.list_jobs(token, &owner, &repository, run_id).await {
            Ok(jobs) => {
                info!(%tag, run = run_id, count = jobs.len(), "tag run jobs read");
                self.tag_jobs.insert(
                    tag.to_owned(),
                    TagJobs {
                        run_id,
                        state: LoadState::Loaded,
                        jobs,
                    },
                );
            }
            Err(error) => {
                warn!(%tag, run = run_id, %error, "could not read the tag run's jobs");
                self.tag_jobs.insert(
                    tag.to_owned(),
                    TagJobs {
                        run_id,
                        state: LoadState::Failed(AppProblem::from_gateway(&error)),
                        jobs: Vec::new(),
                    },
                );
            }
        }
    }

    /// 一个格子：这个 tag 那次运行里属于这个目标的那个 job 走到哪儿了。
    pub fn tag_cell(&self, target: &ReleaseTarget, tag: &str, runs: &[WorkflowRun]) -> BoardCell {
        let run = tag_run(runs, tag);
        let jobs = run.and_then(|run| self.tag_jobs.get(tag).filter(|jobs| jobs.run_id == run.id));
        let job = jobs.and_then(|jobs| job_for_target(&jobs.jobs, &target.name));
        let state = match jobs {
            Some(jobs) if jobs.state == LoadState::Loaded => release_state_for_job(target, job),
            // job 还在路上：先说"构建中"，别把"还没取回来"当成"没构建"。
            Some(_) => ReleaseState::Building,
            None => ReleaseState::Todo,
        };
        BoardCell {
            channel: tag.to_owned(),
            job: job.map(|job| job.name.clone()),
            state,
            run_id: run.map(|run| run.id),
        }
    }

    fn parts(&self) -> Option<(String, String)> {
        self.repository.as_deref().and_then(split_full_name)
    }
}

/// One cell of the board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseCell {
    pub pointer: Option<ChannelPointer>,
    pub state: ReleaseState,
    /// The run this cell's newest dispatch became, when the console has seen it.
    pub run_id: Option<u64>,
}

/// The board as a view wants it: one row per release target, one cell per
/// channel, plus whatever the manifest read had to say.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BoardSnapshot {
    pub manifest_state: ManifestState,
    /// Why the manifest could not be read, when it could not.
    pub problem: Option<String>,
    /// 清单的原文，供编辑器以它为初稿；没读到就是 None。
    pub manifest_text: Option<String>,
    pub rows: Vec<BoardRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardRow {
    pub target: String,
    pub simulated: bool,
    pub cells: Vec<BoardCell>,
}

/// 看板的一格：一个发布目标在一个通道 tag 上的样子。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardCell {
    /// 这一列是哪个通道 tag。
    pub channel: String,
    /// 这个 tag 那次运行里属于这一行的那个 job（模板把它命名成 `目标名 · …`）。
    pub job: Option<String>,
    pub state: ReleaseState,
    /// 这个 tag 触发的运行，控制台认得出就带上——点一下就能去看日志。
    pub run_id: Option<u64>,
}

/// 一个通道 tag 触发的最近一次运行。
///
/// tag push 触发的运行，`head_branch` 就是 tag 名——这就是"这个 tag 现在指着哪次
/// 构建"。
pub fn tag_run<'a>(runs: &'a [WorkflowRun], tag: &str) -> Option<&'a WorkflowRun> {
    runs.iter()
        .filter(|run| run.branch.as_deref() == Some(tag))
        .max_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        })
}

/// 这次运行里属于某个发布目标的 job。
///
/// 模板把 job 命名成 `目标名 · 平台/架构 · 产物`，所以前缀就是发布目标名。
pub fn job_for_target<'a>(jobs: &'a [Job], target: &str) -> Option<&'a Job> {
    jobs.iter()
        .find(|job| job_name_target(&job.name) == Some(target))
}

/// job 名里的发布目标那一段。
fn job_name_target(name: &str) -> Option<&str> {
    name.split_once(" · ").map(|(target, _)| target.trim())
}

impl ReleaseBoard {
    /// Lay the manifest out as a board: one row per release target, one column per
    /// channel tag, and each cell is that target's job in the tag's run.
    pub fn snapshot(&self, runs: &[WorkflowRun]) -> BoardSnapshot {
        let rows = match self.manifest() {
            Some(manifest) => manifest
                .targets()
                .iter()
                .map(|target| BoardRow {
                    target: target.name.clone(),
                    simulated: target.is_simulated(),
                    cells: CHANNELS
                        .iter()
                        .map(|tag| self.tag_cell(target, tag, runs))
                        .collect(),
                })
                .collect(),
            None => Vec::new(),
        };

        BoardSnapshot {
            manifest_state: self.manifest_state,
            problem: self.manifest_problem().map(ToString::to_string),
            manifest_text: self.manifest_text.clone(),
            rows,
        }
    }

    /// What the run produced: the dispatch that asked for it says which version
    /// and target it belongs to, and that is what makes the assets findable.
    pub async fn facts_for_run(
        &self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        run_id: u64,
    ) -> Option<ReleaseFacts> {
        let dispatch = self
            .dispatches
            .iter()
            .find(|dispatch| dispatch.run_id == Some(run_id))?;
        let (owner, repository) = self.parts()?;

        let mut facts = ReleaseFacts {
            version: dispatch.version.clone(),
            target: dispatch.target.clone(),
            config: dispatch.config.clone(),
            assets: Vec::new(),
            pointers: self
                .pointers
                .iter()
                .filter(|pointer| {
                    pointer.target == dispatch.target && pointer.version == dispatch.version
                })
                .cloned()
                .collect(),
            problem: None,
        };

        match gateway
            .release_assets(token, &owner, &repository, &dispatch.version)
            .await
        {
            Ok(assets) => {
                facts.assets = assets
                    .into_iter()
                    .filter(|asset| {
                        version_covers_target(std::slice::from_ref(asset), &dispatch.target)
                    })
                    .collect();
            }
            // 没有 release，或者没有权限看：产物可获取这一条就是"还没有"。
            Err(GatewayError::NotFound) => {}
            Err(error) => facts.problem = Some(AppProblem::from_gateway(&error)),
        }

        Some(facts)
    }

    /// Send the manifest's new text as a pull request: a branch off the default
    /// branch, one commit, one pull request (ADR-0004). The default branch is
    /// never written to.
    pub async fn save_manifest(
        &self,
        gateway: &dyn GitHubGateway,
        token: &SecretToken,
        base_branch: &str,
        contents: &str,
    ) -> Result<ManifestWrite, ManifestWriteProblem> {
        let Some((owner, repository)) = self.parts() else {
            return Err(ManifestWriteProblem::NoRepository);
        };
        let base_branch = base_branch.trim();
        if base_branch.is_empty() {
            return Err(ManifestWriteProblem::NoBase);
        }
        // 先看现在这一份的 revision：有就基于它改，没有就是新建清单。
        let sha = match gateway
            .file_contents(token, &owner, &repository, MANIFEST_PATH)
            .await
        {
            Ok(existing) => Some(existing.sha),
            Err(GatewayError::NotFound) => None,
            Err(error) => {
                return Err(ManifestWriteProblem::Gateway(AppProblem::from_gateway(
                    &error,
                )));
            }
        };

        let branch = format!(
            "release-console/manifest-{}",
            chrono::Utc::now().format("%Y%m%d-%H%M%S")
        );
        gateway
            .create_branch(token, &owner, &repository, &branch, base_branch)
            .await
            .map_err(|error| ManifestWriteProblem::Gateway(AppProblem::from_gateway(&error)))?;

        let message = if sha.is_some() {
            "chore: 更新发布清单".to_owned()
        } else {
            "chore: 添加发布清单".to_owned()
        };
        gateway
            .write_file(
                token,
                &owner,
                &repository,
                FileWrite {
                    path: MANIFEST_PATH.to_owned(),
                    contents: contents.to_owned(),
                    message,
                    reference: branch.clone(),
                    sha,
                },
            )
            .await
            .map_err(|error| ManifestWriteProblem::Gateway(AppProblem::from_gateway(&error)))?;

        let pull_number = gateway
            .open_pull_request(
                token,
                &owner,
                &repository,
                PullRequest {
                    title: "chore: 更新发布清单".to_owned(),
                    body: "由 GitHub Action Console 生成的发布清单改动。\n\n合并后，发布目标与打包配置以这份清单为准（ADR-0003）。"
                        .to_owned(),
                    head: branch.clone(),
                    base: base_branch.to_owned(),
                },
            )
            .await
            .map_err(|error| ManifestWriteProblem::Gateway(AppProblem::from_gateway(&error)))?;

        info!(branch = %branch, pull_number, "manifest sent as a pull request");
        Ok(ManifestWrite {
            branch,
            pull_number,
        })
    }
}

/// Whether a release carries something for a target. The CI names assets after
/// the target they were built for (see the release workflow).
pub fn version_covers_target(assets: &[ReleaseAsset], target: &str) -> bool {
    assets
        .iter()
        .any(|asset| asset.name.to_lowercase().contains(&target.to_lowercase()))
}
