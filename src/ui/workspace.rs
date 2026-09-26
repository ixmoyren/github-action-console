use super::*;
use super::{AppView, RUN_POLL_SECONDS};

use crate::app::{ManifestWrite, ManifestWriteProblem, TriggerProblem};
use crate::release::BuildDispatch;

/// How long the runs drawer takes to slide in, or back out. The drawer travels
/// its own width, so it gets a little longer than a short nudge would.
const DRAWER_SLIDE: Duration = Duration::from_millis(200);

/// Where the runs drawer stands. The drawer slides in, and slides back out the
/// way it came before it leaves the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum DrawerPhase {
    #[default]
    Closed,
    Open,
    /// The slide out is playing; the drawer leaves when it ends.
    Closing,
}

/// What the drawer is showing. The page behind it never changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum DrawerKind {
    #[default]
    Runs,
    Board,
}

impl AppView {
    pub(super) async fn refresh_workspace(
        workspace: &Arc<Mutex<Workspace>>,
        board: &Arc<Mutex<ReleaseBoard>>,
        this: &WeakEntity<AppView>,
        cx: &mut AsyncApp,
    ) {
        let guard = workspace.lock().await;
        let tab = guard.tab();
        let workflows = guard.workflows().to_vec();
        let workflows_state = guard.workflows_state();
        let selected_workflow = guard.selected_workflow_id();
        let workflow_file = guard.workflow_file().map(str::to_owned);
        let workflow_file_state = guard.workflow_file_state();
        let runner_labels = guard.runner_labels().to_vec();
        let runs = guard.runs().to_vec();
        let runs_state = guard.runs_state();
        let runs_has_more = guard.runs_has_more();
        let selected = guard.repository().map(str::to_owned);
        drop(guard);

        // 看板的每一格都由清单、指针和"控制台触发过的构建"拼出来。
        let board_view = {
            let mut guard = board.lock().await;
            guard.bind_dispatches(&runs).await;
            guard.snapshot(&runs)
        };

        if let Err(error) = this.update(cx, |this, cx| {
            this.workspace_tab = tab;
            this.workflows = workflows;
            this.workflows_state = workflows_state;
            this.selected_workflow_id = selected_workflow;
            this.workflow_file = workflow_file;
            this.workflow_file_state = workflow_file_state;
            this.runner_labels = runner_labels;
            this.runs = runs;
            this.runs_state = runs_state;
            this.runs_has_more = runs_has_more;
            this.selected = selected;
            this.board_view = board_view;
            this.refresh_run_table(cx);
            this.poll_runs_if_needed(cx);
            cx.notify();
        }) {
            warn!(?error, "the view was gone before the update landed");
        };
    }
    pub(super) async fn load_workspace(
        gateway: &Arc<dyn GitHubGateway>,
        token: &SecretToken,
        handles: &ScopedHandles,
        runtime: &TokioRuntime,
        this: &WeakEntity<AppView>,
        cx: &mut AsyncApp,
    ) {
        let ScopedHandles {
            picker: _,
            workspace,
            board,
        } = handles;
        let token = token.clone();

        let task = runtime.spawn({
            let gateway = gateway.clone();
            let workspace = workspace.clone();
            let token = token.clone();
            async move {
                workspace
                    .lock()
                    .await
                    .load_workflows(&*gateway, &token)
                    .await;
            }
        });
        if let Err(error) = task.await {
            warn!(%error, "a background task did not finish");
        }

        let task = runtime.spawn({
            let gateway = gateway.clone();
            let workspace = workspace.clone();
            let token = token.clone();
            async move {
                workspace.lock().await.reload_runs(&*gateway, &token).await;
            }
        });
        if let Err(error) = task.await {
            warn!(%error, "a background task did not finish");
        }

        // 发布清单与通道指针随仓库一起读进来。
        let repository = { workspace.lock().await.repository().map(str::to_owned) };
        if let Some(repository) = repository {
            let task = runtime.spawn({
                let gateway = gateway.clone();
                let board = board.clone();
                let token = token.clone();
                async move {
                    let mut guard = board.lock().await;
                    guard.enter(&repository);
                    guard.load(&*gateway, &token).await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
        }

        Self::refresh_workspace(workspace, board, this, cx).await;
    }
    pub(super) fn set_workspace_tab(&mut self, tab: WorkspaceTab, cx: &mut Context<Self>) {
        self.workspace_tab = tab;
        // The drawer follows this immediately; the work behind it can land
        // whenever it lands.
        cx.notify();
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let workspace = self.workspace.clone();
        let board = self.board.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let workspace = workspace.clone();
                async move {
                    workspace.lock().await.set_tab(tab);
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            if tab == WorkspaceTab::Runs {
                let token = { manager.lock().await.token() };
                if let Some(token) = token {
                    let task = runtime.spawn({
                        let gateway = gateway.clone();
                        let workspace = workspace.clone();
                        async move {
                            workspace.lock().await.reload_runs(&*gateway, &token).await;
                        }
                    });
                    if let Err(error) = task.await {
                        warn!(%error, "a background task did not finish");
                    }
                }
            }

            Self::refresh_workspace(&workspace, &board, &this, cx).await;
        })
        .detach();
    }
    pub(super) fn load_more_runs(&mut self, cx: &mut Context<Self>) {
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let workspace = self.workspace.clone();
        let board = self.board.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };
            let task = runtime.spawn({
                let gateway = gateway.clone();
                let workspace = workspace.clone();
                async move {
                    workspace
                        .lock()
                        .await
                        .load_more_runs(&*gateway, &token)
                        .await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
            Self::refresh_workspace(&workspace, &board, &this, cx).await;
        })
        .detach();
    }
    pub(super) fn poll_runs_if_needed(&mut self, cx: &mut Context<Self>) {
        if self.polling {
            return;
        }
        let needed = self.selected.is_some()
            && self.workspace_tab == WorkspaceTab::Runs
            && self.runs.iter().any(|run| run.status.is_running());
        if !needed {
            return;
        }

        self.polling = true;
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let workspace = self.workspace.clone();
        let board = self.board.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            loop {
                let should_poll = { workspace.lock().await.should_poll_runs() };
                if !should_poll {
                    break;
                }

                cx.background_executor()
                    .timer(Duration::from_secs(RUN_POLL_SECONDS))
                    .await;

                let token = { manager.lock().await.token() };
                let Some(token) = token else {
                    break;
                };
                let task = runtime.spawn({
                    let gateway = gateway.clone();
                    let workspace = workspace.clone();
                    async move {
                        workspace.lock().await.reload_runs(&*gateway, &token).await;
                    }
                });
                if let Err(error) = task.await {
                    warn!(%error, "a background task did not finish");
                }

                Self::refresh_workspace(&workspace, &board, &this, cx).await;
            }

            if let Err(error) = this.update(cx, |this, cx| {
                this.polling = false;
                cx.notify();
            }) {
                warn!(?error, "the view was gone before the update landed");
            };
        })
        .detach();
    }
    /// The repository page: the workflow list, the editor, and — when asked for
    /// — the runs drawer over the editor. The drawer never replaces the page.
    pub(super) fn workspace_shell(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .size_full()
            .child(self.workspace_header_ui(cx))
            .child(self.workflows_panel_ui(cx))
            .into_any_element()
    }
    /// The page title and its actions. The runs list is a drawer, so the page
    /// keeps its heading whichever way the drawer is.
    fn workspace_header_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        let link_color = cx.theme().link;
        let (runs_id, runs_label) = if self.drawer_open(DrawerKind::Runs) {
            ("close-runs-drawer", labels::WORKSPACE_RUN_HISTORY_CLOSE)
        } else {
            ("open-runs-drawer", labels::WORKSPACE_RUN_HISTORY)
        };
        let (board_id, board_label) = if self.drawer_open(DrawerKind::Board) {
            ("close-board-drawer", labels::WORKSPACE_BOARD_CLOSE)
        } else {
            ("open-board-drawer", labels::WORKSPACE_BOARD)
        };

        let heading = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(Label::new(labels::WORKSPACE_WORKFLOWS).text_lg())
            .child(
                Button::new(if self.creating_workflow {
                    "new-workflow-off"
                } else {
                    "new-workflow-on"
                })
                .label(if self.creating_workflow {
                    labels::WORKFLOW_NEW_CANCEL
                } else {
                    labels::WORKFLOW_NEW
                })
                .on_click(cx.listener(|this, _, window, cx| this.toggle_new_workflow(window, cx))),
            );

        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(heading)
            // The two drawers open from the far end of the row, away from the
            // page's own actions.
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .id(runs_id)
                            .test_support()
                            .text_color(link_color)
                            .cursor_pointer()
                            .child(runs_label)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_drawer(DrawerKind::Runs, cx);
                            })),
                    )
                    .child(
                        div()
                            .id(board_id)
                            .test_support()
                            .text_color(link_color)
                            .cursor_pointer()
                            .child(board_label)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_drawer(DrawerKind::Board, cx);
                            })),
                    ),
            )
            .into_any_element()
    }
    /// Open or close one of the drawers over the editor.
    pub(super) fn toggle_drawer(&mut self, kind: DrawerKind, cx: &mut Context<Self>) {
        if self.drawer_open(kind) {
            self.close_drawer(cx);
        } else {
            self.open_drawer(kind, cx);
        }
    }
    /// Slide the drawer in from the right, and tell the app layer the runs view
    /// is in front — both drawers read the runs, and polling follows the tab.
    pub(super) fn open_drawer(&mut self, kind: DrawerKind, cx: &mut Context<Self>) {
        self.drawer_kind = kind;
        self.begin_drawer_open(cx);
        self.set_workspace_tab(WorkspaceTab::Runs, cx);
    }
    fn begin_drawer_open(&mut self, cx: &mut Context<Self>) {
        self.set_drawer_phase(DrawerPhase::Open, cx);
    }
    /// Slide the drawer back out to the right, the way it came in, and take it
    /// out of the tree when the slide is done. Polling stops straight away.
    pub(super) fn close_drawer(&mut self, cx: &mut Context<Self>) {
        if self.drawer == DrawerPhase::Closed {
            return;
        }
        self.set_workspace_tab(WorkspaceTab::Workflows, cx);
        self.begin_drawer_close(cx);
    }
    fn begin_drawer_close(&mut self, cx: &mut Context<Self>) {
        if self.drawer == DrawerPhase::Closed {
            return;
        }
        self.set_drawer_phase(DrawerPhase::Closing, cx);

        let generation = self.drawer_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DRAWER_SLIDE).await;
            if let Err(error) = this.update(cx, |this, cx| {
                // A slide that another one overtook leaves the drawer alone.
                if this.drawer_generation == generation && this.drawer == DrawerPhase::Closing {
                    this.drawer = DrawerPhase::Closed;
                    cx.notify();
                }
            }) {
                warn!(?error, "the view was gone before the slide finished");
            }
        })
        .detach();
    }
    fn set_drawer_phase(&mut self, phase: DrawerPhase, cx: &mut Context<Self>) {
        self.drawer = phase;
        self.drawer_generation += 1;
        cx.notify();
    }
    fn runs_drawer_visible(&self) -> bool {
        self.drawer != DrawerPhase::Closed
    }
    /// Whether this drawer is the one on screen, fully in.
    fn drawer_open(&self, kind: DrawerKind) -> bool {
        self.drawer == DrawerPhase::Open && self.drawer_kind == kind
    }
    /// Show a workflow's file. Selecting is cheap; the file itself is fetched
    /// in the background afterwards.
    pub(super) fn select_workflow(&mut self, workflow_id: u64, cx: &mut Context<Self>) {
        // Browsing away from the form means leaving it.
        self.creating_workflow = false;
        // Asking for a workflow's file means seeing the editor: the drawer gets
        // out of the way.
        if self.runs_drawer_visible() {
            self.close_drawer(cx);
        }
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let workspace = self.workspace.clone();
        let board = self.board.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let workspace = workspace.clone();
                async move {
                    workspace.lock().await.select_workflow(workflow_id);
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
            Self::refresh_workspace(&workspace, &board, &this, cx).await;

            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };
            let task = runtime.spawn({
                let gateway = gateway.clone();
                let workspace = workspace.clone();
                async move {
                    workspace
                        .lock()
                        .await
                        .load_workflow_file(&*gateway, &token)
                        .await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
            Self::refresh_workspace(&workspace, &board, &this, cx).await;
        })
        .detach();
    }
    /// Hand the loaded workflow file to the editor.
    ///
    /// The file arrives on a background task, and the editor only takes text
    /// with a window in hand, so the handover happens between frames. The text
    /// is deferred rather than written during render, and the editor is only
    /// touched when the file actually changed.
    pub(super) fn sync_yaml_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // No file (nothing selected, another repository loading, a failure)
        // leaves the editor empty rather than showing the previous workflow.
        let contents = self.workflow_file.clone().unwrap_or_default();
        push_editor_text(
            &self.yaml_editor,
            &mut self.yaml_editor_text,
            contents,
            window,
            cx,
        );

        // The preview editor holds the draft's YAML while the form shows it.
        let preview = if self.previewing_draft {
            self.draft_from_form(cx).to_yaml().unwrap_or_default()
        } else {
            String::new()
        };
        push_editor_text(
            &self.preview_editor,
            &mut self.preview_editor_text,
            preview,
            window,
            cx,
        );

        // 编辑器打开着的时候才喂文本；关着就不动它，免得把草稿覆盖掉。
        if self.editing_manifest {
            let manifest = self.board_view.manifest_text.clone().unwrap_or_default();
            push_editor_text(
                &self.manifest_editor,
                &mut self.manifest_editor_text,
                manifest,
                window,
                cx,
            );
        }

        // The two runner dropdowns: the systems GitHub's runners answer to, and
        // the versions of the chosen one. The self-hosted labels arrive from a
        // background task, and the versions follow whatever system is selected.
        let groups = runner_groups(&self.runner_labels);
        if self.draft_os_options != groups {
            self.draft_os_options = groups.clone();
            let select = self.draft_runner_os.clone();
            window.defer(cx, move |window, cx| {
                select.update(cx, |state, cx| {
                    state.set_items(SearchableVec::new(groups), window, cx);
                });
            });
        }

        let group = self
            .draft_runner_os
            .read(cx)
            .selected_value()
            .map(|group| group.to_string())
            .unwrap_or_else(|| RUNNER_GROUPS[0].to_owned());
        let versions = if group == SELF_HOSTED_GROUP {
            self.runner_labels
                .iter()
                .map(|label| SharedString::from(label.as_str()))
                .collect::<Vec<_>>()
        } else {
            runner_versions_ui(&group)
        };
        if self.draft_version_options != versions {
            self.draft_version_options = versions.clone();
            let select = self.draft_runner_version.clone();
            window.defer(cx, move |window, cx| {
                select.update(cx, |state, cx| {
                    state.set_items(SearchableVec::new(versions), window, cx);
                    // A new system means its first version, not whatever index
                    // the previous system's list happened to have.
                    state.set_selected_index(Some(IndexPath::new(0)), window, cx);
                });
            });
        }

        // The board's target dropdown follows the manifest.
        let targets = self
            .board_view
            .rows
            .iter()
            .map(|row| SharedString::from(row.target.as_str()))
            .collect::<Vec<_>>();
        if self.board_target_options != targets {
            self.board_target_options = targets.clone();
            let select = self.board_target.clone();
            window.defer(cx, move |window, cx| {
                select.update(cx, |state, cx| {
                    state.set_items(SearchableVec::new(targets), window, cx);
                    state.set_selected_index(Some(IndexPath::new(0)), window, cx);
                });
            });
        }
    }
    /// Trigger the selected workflow on the default branch, then pull the runs
    /// so the new run shows up without a manual refresh.
    pub(super) fn run_selected_workflow(&mut self, cx: &mut Context<Self>) {
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let workspace = self.workspace.clone();
        let board = self.board.clone();
        let status = self.status.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };

            let task = runtime.spawn({
                let gateway = gateway.clone();
                let workspace = workspace.clone();
                let status = status.clone();
                let token = token.clone();
                async move {
                    let outcome = workspace
                        .lock()
                        .await
                        .run_selected_workflow(&*gateway, &token)
                        .await;
                    let notice = match outcome {
                        Ok(()) => Notice {
                            kind: NoticeKind::Info,
                            text: labels::WORKFLOW_RUN_TRIGGERED.to_owned(),
                        },
                        Err(RunProblem::NoWorkflow) => Notice {
                            kind: NoticeKind::Warning,
                            text: labels::WORKFLOW_RUN_NO_SELECTION.to_owned(),
                        },
                        Err(RunProblem::NoDefaultBranch) => Notice {
                            kind: NoticeKind::Warning,
                            text: labels::WORKFLOW_RUN_NO_BRANCH.to_owned(),
                        },
                        Err(RunProblem::Gateway(problem)) => notice_for(problem),
                    };
                    status.lock().await.push(notice);

                    if outcome.is_ok() {
                        workspace.lock().await.reload_runs(&*gateway, &token).await;
                    }
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            Self::refresh_status(&status, &this, cx).await;
            Self::refresh_workspace(&workspace, &board, &this, cx).await;
        })
        .detach();
    }
    /// Open or close the new-workflow form. Opening always starts blank.
    pub(super) fn toggle_new_workflow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.creating_workflow {
            self.creating_workflow = false;
            cx.notify();
            return;
        }

        self.reset_draft(window, cx);
        self.creating_workflow = true;
        // The form lives where the drawer would be: give it the pane.
        if self.runs_drawer_visible() {
            self.close_drawer(cx);
        }
        cx.notify();

        // The repository's own runner labels fill the dropdown once GitHub
        // answers; the hosted list is already there.
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let workspace = self.workspace.clone();
        let board = self.board.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };
            let task = runtime.spawn({
                let gateway = gateway.clone();
                let workspace = workspace.clone();
                async move {
                    workspace
                        .lock()
                        .await
                        .load_runner_labels(&*gateway, &token)
                        .await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
            Self::refresh_workspace(&workspace, &board, &this, cx).await;
        })
        .detach();
    }

    /// A blank form: no file name yet, the first runner, no container, one job.
    fn reset_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.draft_file_name
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.draft_name
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.draft_container
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.draft_cron
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.draft_push_branches
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.draft_pull_request_branches
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.draft_runner_os.update(cx, |state, cx| {
            state.set_selected_index(Some(IndexPath::new(0)), window, cx);
        });
        self.draft_runner_version.update(cx, |state, cx| {
            state.set_selected_index(Some(IndexPath::new(0)), window, cx);
        });
        self.draft_manual = true;
        self.draft_push = false;
        self.draft_pull_request = false;
        self.draft_schedule = false;
        self.previewing_draft = false;

        let job = self.new_draft_job(window, cx);
        self.draft_jobs = vec![job];
    }

    fn new_draft_job(&self, window: &mut Window, cx: &mut Context<Self>) -> DraftJobRow {
        DraftJobRow {
            id: cx.new(|cx| draft_input(window, cx, labels::WORKFLOW_NEW_JOB_ID_HINT)),
            name: cx.new(|cx| draft_input(window, cx, labels::WORKFLOW_NEW_JOB_NAME_HINT)),
            command: cx.new(|cx| draft_input(window, cx, labels::WORKFLOW_NEW_JOB_COMMAND_HINT)),
        }
    }

    pub(super) fn add_draft_job(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let job = self.new_draft_job(window, cx);
        self.draft_jobs.push(job);
        cx.notify();
    }

    pub(super) fn remove_draft_job(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.draft_jobs.len() {
            self.draft_jobs.remove(index);
            cx.notify();
        }
    }

    /// What the form says right now, before any of it reaches GitHub.
    fn draft_from_form(&self, cx: &App) -> WorkflowDraft {
        let group = self
            .draft_runner_os
            .read(cx)
            .selected_value()
            .map(|group| group.to_string())
            .unwrap_or_else(|| RUNNER_GROUPS[0].to_owned());
        let version = self
            .draft_runner_version
            .read(cx)
            .selected_value()
            .map(|version| version.to_string())
            .unwrap_or_else(|| {
                runner_versions(&group)
                    .first()
                    .copied()
                    .unwrap_or("latest")
                    .to_owned()
            });

        WorkflowDraft {
            file_name: self.draft_file_name.read(cx).value().to_string(),
            name: self.draft_name.read(cx).value().to_string(),
            runs_on: runner_label(&group, &version),
            container: Some(self.draft_container.read(cx).value().to_string()),
            triggers: Triggers {
                manual: self.draft_manual,
                push: self.draft_push,
                push_branches: branch_patterns(self.draft_push_branches.read(cx).value().as_ref()),
                pull_request: self.draft_pull_request,
                pull_request_branches: branch_patterns(
                    self.draft_pull_request_branches.read(cx).value().as_ref(),
                ),
                schedule: self
                    .draft_schedule
                    .then(|| self.draft_cron.read(cx).value().to_string()),
            },
            jobs: self
                .draft_jobs
                .iter()
                .map(|row| JobDraft {
                    id: row.id.read(cx).value().to_string(),
                    name: row.name.read(cx).value().to_string(),
                    command: row.command.read(cx).value().to_string(),
                })
                .collect(),
        }
    }

    /// Write the draft to a local file, next to the downloads, without telling
    /// GitHub anything.
    pub(super) fn save_draft_locally(&mut self, cx: &mut Context<Self>) {
        let draft = self.draft_from_form(cx);
        let Ok(yaml) = draft.to_yaml() else {
            return;
        };
        let file_name = draft
            .path()
            .map(|path| path.rsplit('/').next().unwrap_or("workflow.yml").to_owned())
            .unwrap_or_else(|_| "workflow.yml".to_owned());

        let downloads = self.downloads.clone();
        let status = self.status.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let status = status.clone();
                async move {
                    let result = downloads.lock().await.save_file(&file_name, &yaml);
                    let notice = match result {
                        Ok(path) => Notice {
                            kind: NoticeKind::Info,
                            text: format!(
                                "{}：{}",
                                labels::WORKFLOW_NEW_SAVED_LOCALLY,
                                path.display()
                            ),
                        },
                        Err(error) => {
                            warn!(%error, "could not save the workflow draft");
                            Notice {
                                kind: NoticeKind::Error,
                                text: labels::WORKFLOW_NEW_SAVE_FAILED.to_owned(),
                            }
                        }
                    };
                    status.lock().await.push(notice);
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            Self::refresh_status(&status, &this, cx).await;
        })
        .detach();
    }

    /// Turn the form into a workflow file, push it, and show it in the editor.
    pub(super) fn create_workflow(&mut self, cx: &mut Context<Self>) {
        let draft = self.draft_from_form(cx);
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let workspace = self.workspace.clone();
        let board = self.board.clone();
        let status = self.status.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };

            let task = runtime.spawn({
                let gateway = gateway.clone();
                let workspace = workspace.clone();
                let status = status.clone();
                let token = token.clone();
                async move {
                    let outcome = workspace
                        .lock()
                        .await
                        .create_workflow(&*gateway, &token, &draft)
                        .await;
                    let notice = match outcome {
                        Ok(PushOutcome::Created) => Notice {
                            kind: NoticeKind::Info,
                            text: labels::WORKFLOW_NEW_PUSHED.to_owned(),
                        },
                        Ok(PushOutcome::Replaced) => Notice {
                            kind: NoticeKind::Info,
                            text: labels::WORKFLOW_NEW_REPLACED.to_owned(),
                        },
                        Err(CreateProblem::Draft(problem)) => Notice {
                            kind: NoticeKind::Warning,
                            text: draft_problem_text(problem).to_owned(),
                        },
                        Err(CreateProblem::NoRepository) => Notice {
                            kind: NoticeKind::Warning,
                            text: labels::WORKFLOW_NEW_NO_REPOSITORY.to_owned(),
                        },
                        Err(CreateProblem::NoDefaultBranch) => Notice {
                            kind: NoticeKind::Warning,
                            text: labels::WORKFLOW_NEW_NO_BRANCH.to_owned(),
                        },
                        Err(CreateProblem::Gateway(problem)) => notice_for(problem),
                    };
                    status.lock().await.push(notice);
                    outcome.is_ok()
                }
            });
            let created = task.await.unwrap_or_else(|error| {
                warn!(%error, "a background task did not finish");
                false
            });

            Self::refresh_status(&status, &this, cx).await;
            Self::refresh_workspace(&workspace, &board, &this, cx).await;

            if created
                && let Err(error) = this.update(cx, |this, cx| {
                    this.creating_workflow = false;
                    this.previewing_draft = false;
                    cx.notify();
                })
            {
                warn!(?error, "the view was gone before the update landed");
            }
        })
        .detach();
    }

    /// Commit what the editor holds, on the repository's default branch.
    pub(super) fn save_workflow_file(&mut self, cx: &mut Context<Self>) {
        let contents = self.yaml_editor.read(cx).value().to_string();
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let workspace = self.workspace.clone();
        let board = self.board.clone();
        let status = self.status.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };

            let task = runtime.spawn({
                let gateway = gateway.clone();
                let workspace = workspace.clone();
                let status = status.clone();
                let token = token.clone();
                async move {
                    let outcome = workspace
                        .lock()
                        .await
                        .save_workflow_file(&*gateway, &token, &contents)
                        .await;
                    let notice = match outcome {
                        Ok(()) => Notice {
                            kind: NoticeKind::Info,
                            text: labels::WORKFLOW_SAVED.to_owned(),
                        },
                        Err(SaveProblem::NoWorkflow) => Notice {
                            kind: NoticeKind::Warning,
                            text: labels::WORKFLOW_RUN_NO_SELECTION.to_owned(),
                        },
                        Err(SaveProblem::NoDefaultBranch) => Notice {
                            kind: NoticeKind::Warning,
                            text: labels::WORKFLOW_SAVE_NO_BRANCH.to_owned(),
                        },
                        Err(SaveProblem::Gateway(problem)) => notice_for(problem),
                    };
                    status.lock().await.push(notice);
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            Self::refresh_status(&status, &this, cx).await;
            Self::refresh_workspace(&workspace, &board, &this, cx).await;
        })
        .detach();
    }
    pub(super) fn workflows_panel_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .gap_4()
            .w_full()
            .flex_1()
            .min_h_0()
            .child(self.workflow_list_ui(cx))
            // The drawer sits over this pane: the workflow page stays where it
            // was, only the editor is covered while the runs are open.
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .h_full()
                    // The drawer slides over this pane and out of it, so the
                    // pane is what clips it.
                    .overflow_hidden()
                    .child(match (self.creating_workflow, self.previewing_draft) {
                        (true, true) => self.workflow_preview_ui(cx),
                        (true, false) => self.workflow_draft_ui(cx),
                        (false, _) => self.workflow_file_ui(cx),
                    })
                    .when(self.runs_drawer_visible(), |this| {
                        this.child(self.runs_drawer_ui(cx))
                    }),
            )
            .into_any_element()
    }
    /// The runs list, sliding in from the right edge of the pane.
    fn runs_drawer_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        let closing = self.drawer == DrawerPhase::Closing;

        div()
            .id("runs-drawer")
            .test_support()
            .absolute()
            .top_0()
            .bottom_0()
            .w_full()
            .child(
                div()
                    // Sliding out is its own timeline, so it can play back the
                    // slide in: the surface starts where it ended and leaves to
                    // the right.
                    .id(if closing {
                        "runs-drawer-out"
                    } else {
                        "runs-drawer-in"
                    })
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .w_full()
                    .occlude()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_3()
                    .bg(cx.theme().background)
                    .border_l_1()
                    .border_color(cx.theme().border)
                    .shadow_xl()
                    // The header's link opens and closes this: the drawer
                    // carries no title bar of its own.
                    .child(match self.drawer_kind {
                        DrawerKind::Runs => self.runs_panel_ui(cx),
                        DrawerKind::Board => self.release_board_ui(cx),
                    })
                    .with_animation(
                        if closing {
                            "runs-drawer-out"
                        } else {
                            "runs-drawer-in"
                        },
                        Animation::new(DRAWER_SLIDE),
                        move |this, delta| {
                            // The drawer is as wide as the pane and travels its
                            // own width: in, it comes from beyond the right edge
                            // until the editor is covered; out, it is pushed back
                            // the same way, which uncovers the editor from the
                            // left as it goes.
                            if closing {
                                this.left(relative(delta))
                            } else {
                                this.left(relative(1. - delta))
                            }
                        },
                    ),
            )
            .into_any_element()
    }
    /// The workflow the file viewer is showing.
    fn selected_workflow(&self) -> Option<&Workflow> {
        let id = self.selected_workflow_id?;
        self.workflows.iter().find(|workflow| workflow.id == id)
    }
    /// The left column: one title per workflow, with the save and run buttons
    /// under them.
    fn workflow_list_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected_color = cx.theme().primary;
        let selected_background = cx.theme().list_active;
        let hover_color = cx.theme().muted;
        let radius = cx.theme().radius;

        let titles = self
            .workflows
            .iter()
            .map(|workflow| {
                let id = workflow.id;
                let selected = self.selected_workflow_id == Some(id);
                div()
                    .id(SharedString::from(format!("workflow-title-{id}")))
                    .test_support()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px_2()
                    .py_1()
                    .rounded(radius)
                    .text_lg()
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover_color))
                    .bg(if selected {
                        selected_background
                    } else {
                        transparent_black()
                    })
                    .when(selected, move |row| {
                        row.font_weight(FontWeight::SEMIBOLD)
                            .text_color(selected_color)
                    })
                    .child(workflow.name.clone())
                    .on_click(cx.listener(move |this, _, _, cx| this.select_workflow(id, cx)))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let mut list = div()
            .flex()
            .flex_col()
            .gap_1()
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .children(titles);

        match self.workflows_state {
            LoadState::Loading if self.workflows.is_empty() => {
                list = list.child(pickable(labels::WORKFLOWS_LOADING));
            }
            LoadState::Failed(problem) => {
                list = list.child(Label::new(problem_text(problem)).text_sm());
            }
            LoadState::Loaded if self.workflows.is_empty() => {
                list = list.child(pickable(labels::WORKFLOWS_EMPTY));
            }
            _ => {}
        }

        let run_button = Button::new("workflow-run")
            .label(labels::WORKFLOW_RUN)
            .disabled(self.selected_workflow_id.is_none())
            .on_click(cx.listener(|this, _, _, cx| this.run_selected_workflow(cx)));

        let save_button = Button::new("workflow-save")
            .label(labels::WORKFLOW_SAVE)
            .disabled(!self.workflow_is_edited(cx))
            .on_click(cx.listener(|this, _, _, cx| this.save_workflow_file(cx)));

        // A new workflow is unaffected by the selected one's save and run.
        let selected_actions = div()
            .flex()
            .flex_row()
            .gap_2()
            .child(save_button)
            .child(run_button);

        div()
            .flex()
            .flex_col()
            .gap_2()
            .w(px(220.))
            .h_full()
            .min_h_0()
            .child(list)
            .when(!self.creating_workflow, |this| this.child(selected_actions))
            .into_any_element()
    }
    /// Whether the editor holds text the repository does not have yet.
    fn workflow_is_edited(&self, cx: &App) -> bool {
        let Some(loaded) = self.workflow_file.as_deref() else {
            return false;
        };
        self.yaml_editor.read(cx).value().as_ref() != loaded
    }
    /// The right column: the selected workflow's name, path, and its file in
    /// the editor.
    fn workflow_file_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(workflow) = self.selected_workflow() else {
            // With no workflow to show, the column explains why it is empty
            // instead of leaving the user to guess.
            let placeholder = match self.workflows_state {
                LoadState::Idle => pickable(labels::WORKFLOWS_PICK).into_any_element(),
                LoadState::Loading => pickable(labels::WORKFLOWS_LOADING).into_any_element(),
                LoadState::Failed(problem) => Label::new(problem_text(problem))
                    .text_sm()
                    .into_any_element(),
                LoadState::Loaded => pickable(labels::WORKFLOWS_EMPTY).into_any_element(),
            };
            return div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(placeholder)
                .into_any_element();
        };

        let header = div()
            .flex()
            .flex_row()
            .items_baseline()
            .gap_2()
            .child(Label::new(workflow.name.clone()).text_lg())
            .child(
                Label::new(workflow.path.clone())
                    .text_sm()
                    .text_color(cx.theme().muted_foreground),
            );

        let body = match (self.workflow_file_state, self.workflow_file.as_deref()) {
            (_, Some("")) => pickable(labels::WORKFLOW_FILE_EMPTY).into_any_element(),
            // Editable: the page's save button commits what the user changes.
            (_, Some(_)) => div()
                .id("workflow-editor-pane")
                .flex_1()
                .min_h_0()
                .min_w_0()
                // The editor sizes to its text on its own, so the page has to
                // say how tall it is.
                .child(Editor::new(&self.yaml_editor).h(relative(1.)))
                .test_support()
                .into_any_element(),
            (LoadState::Loading, None) => {
                pickable(labels::WORKFLOW_FILE_LOADING).into_any_element()
            }
            (LoadState::Failed(problem), None) => Label::new(problem_text(problem))
                .text_sm()
                .into_any_element(),
            _ => div().into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .gap_2()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(header)
            .child(body)
            .into_any_element()
    }
    /// The new-workflow form: what the file is called, where its jobs run, and
    /// what they do. The file is only written when the form is complete.
    fn workflow_draft_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        let draft = self.draft_from_form(cx);
        let problem = draft.to_yaml().err();

        let mut form = div().flex().flex_col().gap_3();
        form = form
            .child(self.draft_field(labels::WORKFLOW_NEW_FILE, &self.draft_file_name))
            .child(self.draft_field(labels::WORKFLOW_NEW_NAME, &self.draft_name))
            .child(self.draft_triggers_ui(cx))
            .child(self.draft_runner_row())
            .child(self.draft_field(labels::WORKFLOW_NEW_CONTAINER, &self.draft_container))
            .child(self.draft_jobs_ui(cx))
            .child(self.draft_submit_ui(problem, cx));

        div()
            .id("workflow-draft-pane")
            .flex()
            .flex_col()
            .gap_3()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(div().flex_1().min_h_0().overflow_y_scrollbar().child(form))
            .test_support()
            .into_any_element()
    }
    fn draft_field(&self, label: &'static str, input: &Entity<InputState>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .max_w(px(420.))
            .child(Label::new(label).text_sm())
            .child(Input::new(input))
            .into_any_element()
    }
    fn draft_runner_row(&self) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(Label::new(labels::WORKFLOW_NEW_RUNNER).text_sm())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .max_w(px(420.))
                    .child(
                        div()
                            .w(px(160.))
                            .child(Select::new(&self.draft_runner_os).id("draft-runner-os")),
                    )
                    .child(
                        div().flex_1().min_w_0().child(
                            Select::new(&self.draft_runner_version).id("draft-runner-version"),
                        ),
                    ),
            )
            .into_any_element()
    }
    /// The triggers the workflow answers to. At least one must stay on, which
    /// the submit button enforces through the draft's own validation.
    fn draft_triggers_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        let checkboxes = div()
            .flex()
            .flex_row()
            .gap_2()
            .child(
                Checkbox::new("trigger-manual")
                    .label(labels::WORKFLOW_NEW_TRIGGER_MANUAL)
                    .checked(self.draft_manual)
                    .on_change(cx.listener(|this, checked, _, cx| {
                        this.draft_manual = *checked;
                        cx.notify();
                    })),
            )
            .child(
                Checkbox::new("trigger-push")
                    .label(labels::WORKFLOW_NEW_TRIGGER_PUSH)
                    .checked(self.draft_push)
                    .on_change(cx.listener(|this, checked, _, cx| {
                        this.draft_push = *checked;
                        cx.notify();
                    })),
            )
            .child(
                Checkbox::new("trigger-pull-request")
                    .label(labels::WORKFLOW_NEW_TRIGGER_PULL_REQUEST)
                    .checked(self.draft_pull_request)
                    .on_change(cx.listener(|this, checked, _, cx| {
                        this.draft_pull_request = *checked;
                        cx.notify();
                    })),
            )
            .child(
                Checkbox::new("trigger-schedule")
                    .label(labels::WORKFLOW_NEW_TRIGGER_SCHEDULE)
                    .checked(self.draft_schedule)
                    .on_change(cx.listener(|this, checked, _, cx| {
                        this.draft_schedule = *checked;
                        cx.notify();
                    })),
            );

        let mut section = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(Label::new(labels::WORKFLOW_NEW_TRIGGERS).text_sm())
            .child(checkboxes);

        // A trigger that can be narrowed asks for its branches right under its
        // checkbox, and a schedule asks for its cron expression.
        if self.draft_push {
            section = section.child(self.draft_branches_ui(
                "draft-push-branches",
                labels::WORKFLOW_NEW_PUSH_BRANCHES,
                &self.draft_push_branches,
            ));
        }
        if self.draft_pull_request {
            section = section.child(self.draft_branches_ui(
                "draft-pull-request-branches",
                labels::WORKFLOW_NEW_PULL_REQUEST_BRANCHES,
                &self.draft_pull_request_branches,
            ));
        }

        if self.draft_schedule {
            section = section.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .max_w(px(420.))
                    .child(Label::new(labels::WORKFLOW_NEW_CRON).text_sm())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&self.draft_cron).id("draft-cron")),
                    ),
            );
        }

        section.into_any_element()
    }
    fn draft_branches_ui(
        &self,
        id: &'static str,
        label: &'static str,
        input: &Entity<InputState>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .max_w(px(420.))
            .child(Label::new(label).text_sm())
            .child(div().flex_1().min_w_0().child(Input::new(input).id(id)))
            .into_any_element()
    }
    fn draft_jobs_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        let rows = self
            .draft_jobs
            .iter()
            .enumerate()
            .map(|(index, job)| {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(div().w(px(120.)).child(Input::new(&job.id)))
                    .child(div().w(px(140.)).child(Input::new(&job.name)))
                    .child(div().flex_1().min_w_0().child(Input::new(&job.command)))
                    .child(
                        Button::new(SharedString::from(format!("draft-job-remove-{index}")))
                            .label(labels::WORKFLOW_NEW_REMOVE_JOB)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove_draft_job(index, cx);
                            })),
                    )
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(Label::new(labels::WORKFLOW_NEW_JOBS).text_sm())
                    .child(
                        Button::new("draft-job-add")
                            .label(labels::WORKFLOW_NEW_ADD_JOB)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_draft_job(window, cx);
                            })),
                    ),
            )
            // Column names for the rows below, so the three inputs are not
            // left to their placeholders alone.
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(120.))
                            .child(Label::new(labels::WORKFLOW_NEW_JOB_ID).text_xs()),
                    )
                    .child(
                        div()
                            .w(px(140.))
                            .child(Label::new(labels::WORKFLOW_NEW_JOB_NAME).text_xs()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child(Label::new(labels::WORKFLOW_NEW_JOB_COMMAND).text_xs()),
                    ),
            )
            .children(rows)
            .into_any_element()
    }
    fn draft_submit_ui(&self, problem: Option<DraftProblem>, cx: &mut Context<Self>) -> AnyElement {
        // Saving to disk and previewing only need a draft that is a workflow;
        // pushing is the step that writes to GitHub, replacing a file of the
        // same name if the repository already has one.
        let ready = problem.is_none();
        let mut submit = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .child(
                Button::new("draft-save")
                    .label(labels::WORKFLOW_NEW_SAVE)
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, _, cx| this.save_draft_locally(cx))),
            )
            .child(
                Button::new("draft-preview")
                    .label(labels::WORKFLOW_NEW_PREVIEW)
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.previewing_draft = true;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("draft-push")
                    .label(labels::WORKFLOW_NEW_PUSH)
                    .primary()
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, _, cx| this.create_workflow(cx))),
            );

        if let Some(problem) = problem {
            submit = submit.child(
                Label::new(draft_problem_text(problem))
                    .text_sm()
                    .text_color(cx.theme().danger),
            );
        }

        submit.into_any_element()
    }
    /// The generated file, in a read-only editor, before anything is written
    /// anywhere.
    fn workflow_preview_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("workflow-preview-pane")
            .flex()
            .flex_col()
            .gap_2()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("draft-preview-back")
                            .label(labels::WORKFLOW_NEW_BACK)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.previewing_draft = false;
                                cx.notify();
                            })),
                    )
                    .child(Label::new(labels::WORKFLOW_NEW_PREVIEW).text_sm()),
            )
            .child(
                div().flex_1().min_h_0().min_w_0().child(
                    Editor::new(&self.preview_editor)
                        .h(relative(1.))
                        .readonly(true),
                ),
            )
            .test_support()
            .into_any_element()
    }
    pub(super) fn runs_panel_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.open_run.is_some() {
            return self.run_detail_ui(cx);
        }

        let rows = self.run_rows(cx);
        let mut panel = div().flex().flex_col().gap_3().flex_1().min_h_0().w_full();

        // The branch filter narrows the table's rows; the rest of the old
        // filter row is gone, so sorting is what picks a column out now.
        panel = panel.child(
            div()
                .max_w(px(320.))
                .child(Input::new(&self.branch_input).id("runs-branch-filter")),
        );

        if rows > 0 {
            panel = panel.child(div().flex_1().min_h_0().child(self.run_table_ui()));
        } else {
            let message = match self.runs_state {
                LoadState::Loading => Some(labels::RUNS_LOADING),
                LoadState::Failed(problem) => Some(problem_text(problem)),
                LoadState::Loaded => Some(labels::RUNS_EMPTY),
                LoadState::Idle => None,
            };
            if let Some(message) = message {
                panel = panel.child(pickable(message));
            }
        }

        if self.runs_has_more {
            panel = panel.child(
                div().flex().flex_row().justify_end().child(
                    Button::new("runs-load-more")
                        .label(labels::RUNS_LOAD_MORE)
                        .on_click(cx.listener(|this, _, _, cx| this.load_more_runs(cx))),
                ),
            );
        }

        panel.into_any_element()
    }
    /// The release board: one row per release target, one column per channel.
    /// Each cell says where that channel points, how far the release has got,
    /// and offers the one explicit action that can move a pointer.
    fn release_board_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        // 改清单是同一个抽屉里的另一面：抽屉还是这个抽屉，内容换成编辑器。
        if self.editing_manifest {
            return self.manifest_editor_ui(cx);
        }
        let Some(version) = self.board_version_value(cx) else {
            // 没有版本就没法触发，也没法发布；表格照旧显示。
            return self.board_table_ui(None, cx);
        };
        self.board_table_ui(Some(version), cx)
    }

    /// 清单编辑器：在抽屉里改发布清单，提交时落到新分支并开成 PR，默认分支不直接写。
    fn manifest_editor_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(Label::new(labels::BOARD_MANIFEST_TITLE).text_lg())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("board-manifest-submit")
                            .label(labels::BOARD_MANIFEST_SUBMIT)
                            .on_click(cx.listener(|this, _, _, cx| this.submit_manifest(cx))),
                    )
                    .child(
                        Button::new("board-manifest-cancel")
                            .label(labels::BOARD_MANIFEST_CANCEL)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.toggle_manifest_editor(cx)),
                            ),
                    ),
            );

        // 没清单就是新的一份，读不出来就把原因写在编辑器上面。
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_3()
            .flex_1()
            .min_h_0()
            .child(header);
        match self.board_view.manifest_state {
            ManifestState::Missing => {
                panel = panel.child(pickable(labels::BOARD_MISSING));
            }
            ManifestState::Invalid => {
                panel = panel.child(
                    Label::new(format!(
                        "{}：{}",
                        labels::BOARD_INVALID,
                        self.board_view.problem.as_deref().unwrap_or_default()
                    ))
                    .text_sm(),
                );
            }
            _ => {}
        }

        div()
            .id("board-manifest-editor")
            .test_support()
            .flex()
            .flex_1()
            .min_h_0()
            .child(
                panel.child(
                    div()
                        .id("board-manifest-pane")
                        .test_support()
                        .flex_1()
                        .min_h_0()
                        .min_w_0()
                        // 编辑器按自己的文字量排高度，高度得由这一层说了算。
                        .child(Editor::new(&self.manifest_editor).h(relative(1.))),
                ),
            )
            .into_any_element()
    }

    fn board_version_value(&self, cx: &App) -> Option<String> {
        let version = self.board_version.read(cx).value().trim().to_owned();
        (!version.is_empty()).then_some(version)
    }

    fn board_table_ui(&self, version: Option<String>, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = div().flex().flex_col().gap_3();

        // 触发区：版本 + 目标 + 触发。
        panel = panel.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(Label::new(labels::BOARD_VERSION).text_sm())
                        .child(
                            div()
                                .w(px(160.))
                                .child(Input::new(&self.board_version).id("board-version")),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(Label::new(labels::BOARD_TARGET).text_sm())
                        .child(
                            div()
                                .w(px(180.))
                                .child(Select::new(&self.board_target).id("board-target")),
                        ),
                )
                .child(
                    Button::new("board-trigger")
                        .label(labels::BOARD_TRIGGER)
                        .disabled(version.is_none())
                        .on_click(cx.listener(|this, _, _, cx| this.trigger_build_from_board(cx))),
                )
                .child(
                    // 清单本身也在抽屉里改：挪到编辑器那一面，提交时开 PR。
                    Button::new("board-manifest-edit")
                        .label(labels::BOARD_MANIFEST_EDIT)
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_manifest_editor(cx))),
                ),
        );

        match self.board_view.manifest_state {
            ManifestState::Loading => panel = panel.child(pickable(labels::BOARD_LOADING)),
            ManifestState::Missing => panel = panel.child(pickable(labels::BOARD_MISSING)),
            ManifestState::Invalid => {
                panel = panel.child(
                    Label::new(format!(
                        "{}：{}",
                        labels::BOARD_INVALID,
                        self.board_view.problem.as_deref().unwrap_or_default()
                    ))
                    .text_sm(),
                )
            }
            ManifestState::Failed(problem) => {
                panel = panel.child(Label::new(problem_text(problem)).text_sm());
            }
            _ => {}
        }

        // 表头：目标 + 各通道。
        let mut header = div().flex().flex_row().items_center().gap_2().child(
            div()
                .w(px(160.))
                .child(Label::new(labels::BOARD_TARGET).text_sm()),
        );
        for channel in CHANNELS {
            header = header.child(
                div()
                    .w(px(200.))
                    .child(Label::new(channel.to_uppercase()).text_sm()),
            );
        }
        panel = panel.child(header);

        let rows = self
            .board_view
            .rows
            .iter()
            .map(|row| {
                let mut cells = div().flex().flex_row().items_center().gap_2().child(
                    div()
                        .w(px(160.))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(Label::new(row.target.clone()))
                        .when(row.simulated, |this| {
                            this.child(Label::new(labels::BOARD_SIMULATED).text_xs())
                        }),
                );
                for cell in &row.cells {
                    let target = row.target.clone();
                    let channel = cell.channel.clone();
                    let publish = version.clone().filter(|_| cell.version.is_none());
                    let run_id = cell.run_id;

                    cells = cells.child(
                        div()
                            .w(px(200.))
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(pickable(
                                cell.version
                                    .clone()
                                    .unwrap_or_else(|| labels::BOARD_EMPTY_CELL.to_owned()),
                            ))
                            .child(Label::new(cell.state.label()).text_xs())
                            .when_some(publish, |this, _| {
                                this.child(
                                    Button::new(SharedString::from(format!(
                                        "board-publish-{target}-{channel}"
                                    )))
                                    .xsmall()
                                    .label(labels::BOARD_PUBLISH)
                                    .disabled(run_id.is_none() && cell.version.is_none())
                                    .on_click(cx.listener({
                                        let target = target.clone();
                                        let channel = channel.clone();
                                        move |this, _, _, cx| {
                                            this.publish_to_channel(
                                                target.clone(),
                                                channel.clone(),
                                                cx,
                                            );
                                        }
                                    })),
                                )
                            }),
                    );
                }
                div().flex().flex_row().items_center().gap_2().child(cells)
            })
            .collect::<Vec<_>>();

        panel = panel.child(div().flex().flex_col().gap_2().children(rows));
        div()
            .id("board-drawer")
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(div().flex_1().min_h_0().overflow_y_scrollbar().child(panel))
            .into_any_element()
    }

    /// Trigger a build for the chosen target with the chosen version.
    pub(super) fn trigger_build_from_board(&mut self, cx: &mut Context<Self>) {
        let Some(version) = self.board_version_value(cx) else {
            self.push_board_notice(labels::BOARD_NO_VERSION, NoticeKind::Warning, cx);
            return;
        };
        let Some(target) = self
            .board_target
            .read(cx)
            .selected_value()
            .map(|name| name.to_string())
        else {
            self.push_board_notice(labels::BOARD_TARGET, NoticeKind::Warning, cx);
            return;
        };

        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let board = self.board.clone();
        let workspace = self.workspace.clone();
        let status = self.status.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };
            let task = runtime.spawn({
                let gateway = gateway.clone();
                let board = board.clone();
                let workspace = workspace.clone();
                let status = status.clone();
                let token = token.clone();
                async move {
                    let outcome = board
                        .lock()
                        .await
                        .trigger(&*gateway, &token, &target, &version, None)
                        .await;
                    let notice = trigger_notice(outcome);
                    status.lock().await.push(notice);
                    // 触发之后顺手把运行列表刷新一遍，好让这次 dispatch 认领到它的 run。
                    workspace.lock().await.reload_runs(&*gateway, &token).await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            Self::refresh_status(&status, &this, cx).await;
            Self::refresh_workspace(&workspace, &board, &this, cx).await;
        })
        .detach();
    }

    /// 在看板和清单编辑器之间来回。关掉编辑器就把手上的草稿丢掉，下次从仓库那份重新起步。
    pub(super) fn toggle_manifest_editor(&mut self, cx: &mut Context<Self>) {
        self.editing_manifest = !self.editing_manifest;
        if !self.editing_manifest {
            self.manifest_editor_text = None;
        }
        cx.notify();
    }

    /// 把编辑器里的清单交上去：落新分支、开 PR。默认分支只当基线，从不直接写。
    pub(super) fn submit_manifest(&mut self, cx: &mut Context<Self>) {
        let contents = self.manifest_editor.read(cx).value().to_string();
        if contents.trim().is_empty() {
            self.push_board_notice(labels::BOARD_MANIFEST_NO_TEXT, NoticeKind::Warning, cx);
            return;
        }
        if self.selected.is_none() {
            self.push_board_notice(
                labels::BOARD_MANIFEST_NO_REPOSITORY,
                NoticeKind::Warning,
                cx,
            );
            return;
        }

        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let board = self.board.clone();
        let workspace = self.workspace.clone();
        let status = self.status.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };
            // PR 的基线是仓库的默认分支，得先把仓库那一侧的答案拿过来。
            let base = { workspace.lock().await.default_branch().map(str::to_owned) };
            let Some(base) = base else {
                let notice = Notice {
                    kind: NoticeKind::Warning,
                    text: labels::BOARD_MANIFEST_NO_BASE.to_owned(),
                };
                let task = runtime.spawn({
                    let status = status.clone();
                    async move {
                        status.lock().await.push(notice);
                    }
                });
                if let Err(error) = task.await {
                    warn!(%error, "a background task did not finish");
                }
                Self::refresh_status(&status, &this, cx).await;
                return;
            };

            let task = runtime.spawn({
                let gateway = gateway.clone();
                let board = board.clone();
                let status = status.clone();
                let token = token.clone();
                async move {
                    let outcome = board
                        .lock()
                        .await
                        .save_manifest(&*gateway, &token, &base, &contents)
                        .await;
                    status.lock().await.push(manifest_write_notice(outcome));
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            Self::refresh_status(&status, &this, cx).await;
            Self::refresh_workspace(&workspace, &board, &this, cx).await;
        })
        .detach();
    }

    /// A one-off notice from the board, without a board call behind it.
    fn push_board_notice(&mut self, text: &str, kind: NoticeKind, cx: &mut Context<Self>) {
        let status = self.status.clone();
        let notice = Notice {
            kind,
            text: text.to_owned(),
        };
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let status = status.clone();
                async move {
                    status.lock().await.push(notice);
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
            Self::refresh_status(&status, &this, cx).await;
        })
        .detach();
    }
    /// Point one channel at the version in the trigger field.
    pub(super) fn publish_to_channel(
        &mut self,
        target: String,
        channel: String,
        cx: &mut Context<Self>,
    ) {
        let Some(version) = self.board_version_value(cx) else {
            self.push_board_notice(labels::BOARD_NO_VERSION, NoticeKind::Warning, cx);
            return;
        };

        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let board = self.board.clone();
        let workspace = self.workspace.clone();
        let status = self.status.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };
            let task = runtime.spawn({
                let gateway = gateway.clone();
                let board = board.clone();
                let status = status.clone();
                let token = token.clone();
                async move {
                    let outcome = board
                        .lock()
                        .await
                        .publish(&*gateway, &token, &target, &channel, &version)
                        .await;
                    status.lock().await.push(publish_notice(outcome));
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            Self::refresh_status(&status, &this, cx).await;
            Self::refresh_workspace(&workspace, &board, &this, cx).await;
        })
        .detach();
    }
}

/// What to tell the user about a form that is not a workflow yet.
/// The notice a trigger attempt earns.
fn trigger_notice(outcome: Result<BuildDispatch, TriggerProblem>) -> Notice {
    let (kind, text) = match outcome {
        Ok(_) => (NoticeKind::Info, labels::BOARD_TRIGGERED),
        Err(TriggerProblem::NoManifest) => (NoticeKind::Warning, labels::BOARD_TRIGGER_NO_MANIFEST),
        Err(TriggerProblem::UnknownTarget) => {
            (NoticeKind::Warning, labels::BOARD_TRIGGER_UNKNOWN_TARGET)
        }
        Err(TriggerProblem::NoRepository) => (NoticeKind::Warning, labels::BOARD_MISSING),
        Err(TriggerProblem::Store) => (NoticeKind::Error, labels::BOARD_STORE_FAILED),
        Err(TriggerProblem::Gateway(problem)) => {
            return notice_for(problem);
        }
    };
    Notice {
        kind,
        text: text.to_owned(),
    }
}

/// 提交清单这件事的结果：新分支和 PR 号写在通知里，好让人能去仓库里接着看。
fn manifest_write_notice(outcome: Result<ManifestWrite, ManifestWriteProblem>) -> Notice {
    match outcome {
        Ok(write) => Notice {
            kind: NoticeKind::Info,
            text: format!(
                "{}：{}（PR #{}）",
                labels::BOARD_MANIFEST_SUBMITTED,
                write.branch,
                write.pull_number
            ),
        },
        Err(ManifestWriteProblem::NoRepository) => Notice {
            kind: NoticeKind::Warning,
            text: labels::BOARD_MANIFEST_NO_REPOSITORY.to_owned(),
        },
        Err(ManifestWriteProblem::NoBase) => Notice {
            kind: NoticeKind::Warning,
            text: labels::BOARD_MANIFEST_NO_BASE.to_owned(),
        },
        Err(ManifestWriteProblem::Gateway(problem)) => notice_for(problem),
    }
}

/// The notice a publish attempt earns.
fn publish_notice(outcome: Result<ChannelPointer, PublishProblem>) -> Notice {
    let (kind, text) = match outcome {
        Ok(_) => (NoticeKind::Info, labels::BOARD_PUBLISHED),
        Err(PublishProblem::NothingToPublish) => {
            (NoticeKind::Warning, labels::BOARD_PUBLISH_NEEDS_ASSETS)
        }
        Err(PublishProblem::UnknownTarget) => {
            (NoticeKind::Warning, labels::BOARD_PUBLISH_UNKNOWN_TARGET)
        }
        Err(PublishProblem::NoManifest) => (NoticeKind::Warning, labels::BOARD_MISSING),
        Err(PublishProblem::NoRepository) => (NoticeKind::Warning, labels::BOARD_MISSING),
        Err(PublishProblem::Store) => (NoticeKind::Error, labels::BOARD_STORE_FAILED),
        Err(PublishProblem::Gateway(problem)) => {
            return notice_for(problem);
        }
    };
    Notice {
        kind,
        text: text.to_owned(),
    }
}

fn draft_problem_text(problem: DraftProblem) -> &'static str {
    match problem {
        DraftProblem::FileName => labels::WORKFLOW_NEW_BAD_FILE,
        DraftProblem::JobId => labels::WORKFLOW_NEW_BAD_JOB,
        DraftProblem::NoJobs => labels::WORKFLOW_NEW_NO_JOBS,
        DraftProblem::NoTrigger => labels::WORKFLOW_NEW_NO_TRIGGER,
        DraftProblem::Schedule => labels::WORKFLOW_NEW_BAD_SCHEDULE,
    }
}

/// Hand text to an editor state. Editors only take text with a window in hand,
/// and what they show arrives from a background task, so the handover happens
/// between frames and only when the text actually changed.
fn push_editor_text(
    editor: &Entity<EditorState>,
    applied: &mut Option<String>,
    contents: String,
    window: &mut Window,
    cx: &mut Context<AppView>,
) {
    if applied.as_deref() == Some(contents.as_str()) {
        return;
    }
    *applied = Some(contents.clone());

    let editor = editor.clone();
    window.defer(cx, move |window, cx| {
        editor.update(cx, |state, cx| {
            if state.value().as_ref() != contents.as_str() {
                state.set_value(contents, window, cx);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    use gpui_kit::Entity;
    use gpui_kit::component::input::EditorState;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{AppContext as _, TestAppContext, WindowHandle, px, size};
    use tokio::sync::Mutex;

    use crate::app::{
        AuthManager, AuthState, Downloads, LoadState, RepositoryList, RunDetail, Status, Workspace,
    };
    use crate::github::client::OctocrabGateway;
    use crate::github::{Account, GitHubGateway, RunStatus, Workflow, WorkflowRun};
    use crate::runtime::TokioRuntime;
    use crate::store::Store;

    use crate::app::{BoardCell, BoardRow, BoardSnapshot, ManifestState};
    use crate::release::ReleaseState;

    use super::{AppView, CHANNELS, DrawerKind, ReleaseBoard, Root, Services};

    /// One run of the harness repository's workflow, with the fields the table
    /// shows.
    fn run(id: u64, workflow_id: u64, branch: &str, conclusion: &str) -> WorkflowRun {
        WorkflowRun {
            id,
            workflow_id,
            name: "ci".to_owned(),
            status: RunStatus::Completed,
            conclusion: Some(conclusion.to_owned()),
            branch: Some(branch.to_owned()),
            event: "push".to_owned(),
            actor: Some("octocat".to_owned()),
            created_at: Some("2026-09-22T10:00:00Z".to_owned()),
            html_url: Some(format!("https://github.com/octo/alpha/actions/runs/{id}")),
        }
    }

    /// A signed-in view sitting on an open repository with one workflow, which
    /// is the state the workflow page is designed for.
    fn workspace_page(
        cx: &mut TestAppContext,
    ) -> (WindowHandle<Root>, Entity<AppView>, Entity<EditorState>) {
        cx.update(gpui_kit::init);
        // Slides and other animations settle at once here, so a test says what
        // it means without sleeping through them.
        cx.update(|cx| cx.set_reduce_motion(true));

        let runtime = TokioRuntime::new().expect("a tokio runtime");
        let store = runtime
            .block_on(Store::in_memory())
            .expect("an in-memory store");
        let gateway: Arc<dyn GitHubGateway> = Arc::new(OctocrabGateway::new());
        let services = Services {
            manager: Arc::new(Mutex::new(AuthManager::new(gateway.clone(), store.clone()))),
            picker: Arc::new(Mutex::new(RepositoryList::new(
                gateway.clone(),
                store.clone(),
            ))),
            workspace: Arc::new(Mutex::new(Workspace::new())),
            detail: Arc::new(Mutex::new(RunDetail::new())),
            downloads: Arc::new(Mutex::new(Downloads::new(std::env::temp_dir()))),
            board: Arc::new(Mutex::new(ReleaseBoard::new(store.clone()))),
            status: Arc::new(Mutex::new(Status::new())),
            gateway,
            store,
            initial_proxy: None,
            runtime,
        };

        let captured = Rc::new(RefCell::new(None));
        let slot = captured.clone();
        let handle = cx.open_window(size(px(1200.), px(800.)), move |window, cx| {
            let view = cx.new(|cx| AppView::new(services, window, cx));
            *slot.borrow_mut() = Some(view.clone());
            view.update(cx, |view, cx| {
                // The real app wires this up at launch; the runs table's branch
                // filter needs it too.
                view.wire(cx);
                view.auth = AuthState::Authenticated {
                    account: Account {
                        login: "octocat".to_owned(),
                    },
                };
                view.selected = Some("octo/alpha".to_owned());
                view.workflows = vec![Workflow {
                    id: 1,
                    name: "ci".to_owned(),
                    path: ".github/workflows/ci.yml".to_owned(),
                }];
                view.selected_workflow_id = Some(1);
                view.workflows_state = LoadState::Loaded;
                view.workflow_file = Some("name: ci\non: push\n".to_owned());
                view.workflow_file_state = LoadState::Loaded;
                view.runs = vec![
                    run(1, 1, "main", "success"),
                    run(2, 1, "release/1.0", "failure"),
                ];
                view.runs_state = LoadState::Loaded;
                view.refresh_run_table(cx);
                cx.notify();
            });
            Root::new(view, window, cx)
        });

        let view = captured.borrow_mut().take().expect("the view");
        let editor = view.read_with(cx, |view, _| view.yaml_editor.clone());
        (handle, view, editor)
    }

    #[gpui_kit::test]
    fn the_editor_fills_the_workflow_page(cx: &mut TestAppContext) {
        let (handle, _view, editor) = workspace_page(cx);

        cx.update_window(handle.into(), |_, window, cx| {
            // One frame hands the file to the editor, the next draws it.
            window.draw(cx).clear(cx);
            window.draw(cx).clear(cx);

            let editor = window.find(("input", editor.entity_id()));
            assert!(editor.visible(), "the editor is not visible");
            // The editor sizes to its text unless the page says otherwise, so
            // a missing height shows up here as a couple of rows.
            assert!(
                editor.bounds().size.height >= px(560.),
                "the editor itself is {} high in an 800px window",
                editor.bounds().size.height
            );
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn the_save_button_follows_the_editors_text(cx: &mut TestAppContext) {
        let (handle, view, editor) = workspace_page(cx);
        let handle = handle.into();

        // The file reaches the editor through a deferred update, so the page
        // needs more than one frame before it is showing the file.
        for _ in 0..2 {
            cx.update_window(handle, |_, window, cx| {
                window.draw(cx).clear(cx);
            })
            .unwrap();
        }

        cx.update_window(handle, |_, window, cx| {
            assert_eq!(
                editor.read(cx).value().as_ref(),
                "name: ci\non: push\n",
                "the editor never received the file"
            );
            // Nothing is changed yet, so there is nothing to save.
            assert!(!view.read_with(cx, |view, cx| view.workflow_is_edited(cx)));

            // An edit offers a save.
            editor.update(cx, |state, cx| {
                state.set_value("name: ci\non: [push]\n", window, cx);
            });
            window.draw(cx).clear(cx);
            assert!(view.read_with(cx, |view, cx| view.workflow_is_edited(cx)));
        })
        .unwrap();

        // A later frame keeps the edit instead of re-syncing the loaded file.
        cx.update_window(handle, |_, window, cx| {
            window.draw(cx).clear(cx);
            assert_eq!(
                editor.read(cx).value().as_ref(),
                "name: ci\non: [push]\n",
                "a later frame overwrote the edit"
            );
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn the_new_workflow_button_opens_and_closes_the_form(cx: &mut TestAppContext) {
        let (handle, view, _editor) = workspace_page(cx);
        let handle = handle.into();

        cx.update_window(handle, |_, window, cx| {
            window.draw(cx).clear(cx);
            assert!(
                window.try_find("workflow-draft-pane").is_none(),
                "the form is open before it was asked for"
            );

            window.click("new-workflow-on", cx);
            window.draw(cx).clear(cx);
            assert!(window.find("workflow-draft-pane").visible());

            // Fill in enough of the draft for the buttons to come alive.
            let (file_name, job_id) = view.read_with(cx, |view, _| {
                (view.draft_file_name.clone(), view.draft_jobs[0].id.clone())
            });
            file_name.update(cx, |state, cx| state.set_value("nightly", window, cx));
            job_id.update(cx, |state, cx| state.set_value("build", window, cx));
            window.draw(cx).clear(cx);
            let draft = view.read_with(cx, |view, cx| view.draft_from_form(cx));
            assert!(draft.to_yaml().is_ok(), "{:?}", draft.to_yaml().err());

            // Jobs can be added to the draft while it is open.
            window.click("draft-job-add", cx);
            window.draw(cx).clear(cx);
            assert!(
                window.try_find("draft-job-remove-1").is_some(),
                "the second job row never appeared"
            );
            // A blank job leaves the draft incomplete, so name it too.
            let second_job = view.read_with(cx, |view, _| view.draft_jobs[1].id.clone());
            second_job.update(cx, |state, cx| state.set_value("package", window, cx));
            window.draw(cx).clear(cx);

            // The two runner dropdowns start on the hosted default.
            assert_eq!(window.find("draft-runner-os").value(), Some("ubuntu"));
            assert_eq!(window.find("draft-runner-version").value(), Some("latest"));

            // The trigger row starts on the console's own trigger, and the
            // narrower triggers only ask for their input once they are on.
            assert!(window.try_find("trigger-manual").is_some());
            assert!(window.try_find("draft-cron").is_none());
            assert!(window.try_find("draft-push-branches").is_none());
            window.click("trigger-push", cx);
            window.draw(cx).clear(cx);
            assert!(view.read_with(cx, |view, _| view.draft_push));
            assert!(window.try_find("draft-push-branches").is_some());
            window.click("trigger-schedule", cx);
            window.draw(cx).clear(cx);
            assert!(
                window.try_find("draft-cron").is_some(),
                "the cron field did not appear"
            );
            window.click("trigger-schedule", cx);
            window.draw(cx).clear(cx);

            // The version list follows the system that is chosen.
            let os = view.read_with(cx, |view, _| view.draft_runner_os.clone());
            os.update(cx, |state, cx| {
                state.set_selected_index(Some(gpui_kit::component::IndexPath::new(1)), window, cx);
            });
            window.draw(cx).clear(cx);
            assert_eq!(
                view.read_with(cx, |view, _| view.draft_version_options.clone()),
                ["latest", "2025", "2022"]
                    .into_iter()
                    .map(gpui_kit::SharedString::from)
                    .collect::<Vec<_>>()
            );

            // Previewing swaps the form for the generated file.
            window.click("draft-preview", cx);
            window.draw(cx).clear(cx);
            assert!(window.find("workflow-preview-pane").visible());
            window.click("draft-preview-back", cx);
            window.draw(cx).clear(cx);
            assert!(window.find("workflow-draft-pane").visible());

            window.click("new-workflow-off", cx);
            window.draw(cx).clear(cx);
            assert!(window.try_find("workflow-draft-pane").is_none());
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn the_board_drawer_shows_every_target_and_channel(cx: &mut TestAppContext) {
        use gpui_kit::AppContext as _;

        let (handle, view, _editor) = workspace_page(cx);
        let handle: gpui_kit::AnyWindowHandle = handle.into();

        // A manifest with two targets: one plain, one with simulated steps.
        view.update(cx, |view, cx| {
            view.board_view = BoardSnapshot {
                manifest_state: ManifestState::Loaded,
                problem: None,
                manifest_text: Some("targets:\n  - web-arm\n".to_owned()),
                rows: vec![
                    BoardRow {
                        target: "web-arm".to_owned(),
                        simulated: false,
                        cells: CHANNELS
                            .iter()
                            .map(|channel| BoardCell {
                                channel: (*channel).to_owned(),
                                version: (*channel == "latest").then(|| "v0.1.0".to_owned()),
                                state: if *channel == "latest" {
                                    ReleaseState::Published
                                } else {
                                    ReleaseState::Todo
                                },
                                run_id: None,
                            })
                            .collect(),
                    },
                    BoardRow {
                        target: "mas".to_owned(),
                        simulated: true,
                        cells: CHANNELS
                            .iter()
                            .map(|channel| BoardCell {
                                channel: (*channel).to_owned(),
                                version: None,
                                state: ReleaseState::AwaitingSigning,
                                run_id: Some(7),
                            })
                            .collect(),
                    },
                ],
            };
            view.begin_drawer_open(cx);
            view.drawer_kind = DrawerKind::Board;
            cx.notify();
        });

        cx.update_window(handle, |_, window, cx| {
            // 触发/发布都以上面那个版本字段为准，先填一个。
            let version = view.read_with(cx, |view, _| view.board_version.clone());
            version.update(cx, |state, cx| state.set_value("v0.1.0", window, cx));
            window.draw(cx).clear(cx);

            assert!(window.find("board-drawer").visible());
            // 触发区：版本、目标与触发按钮都在。
            assert!(window.try_find("board-version").is_some());
            assert!(window.try_find("board-target").is_some());
            assert!(window.try_find("board-trigger").is_some());
            // 每个有指针的格子给出发布入口，没有被指到的通道也给一个。
            assert!(window.try_find("board-publish-web-arm-lts").is_some());
            assert!(window.try_find("board-publish-mas-latest").is_some());
            // The workflow page is still there behind the drawer.
            assert!(window.try_find("workflow-title-1").is_some());
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn the_manifest_editor_opens_over_the_board_and_seeds_the_repo_text(cx: &mut TestAppContext) {
        use gpui_kit::AppContext as _;

        let (handle, view, _editor) = workspace_page(cx);
        let handle: gpui_kit::AnyWindowHandle = handle.into();

        view.update(cx, |view, cx| {
            view.board_view = BoardSnapshot {
                manifest_state: ManifestState::Loaded,
                problem: None,
                manifest_text: Some("targets:\n  - web-arm\n".to_owned()),
                rows: vec![BoardRow {
                    target: "web-arm".to_owned(),
                    simulated: false,
                    cells: CHANNELS
                        .iter()
                        .map(|channel| BoardCell {
                            channel: (*channel).to_owned(),
                            version: None,
                            state: ReleaseState::Todo,
                            run_id: None,
                        })
                        .collect(),
                }],
            };
            view.begin_drawer_open(cx);
            view.drawer_kind = DrawerKind::Board;
            cx.notify();
        });

        cx.update_window(handle, |_, window, cx| {
            window.draw(cx).clear(cx);
            assert!(window.find("board-drawer").visible());
            // 看板那一面先显示，编辑器要点了才出来。
            assert!(window.try_find("board-manifest-editor").is_none());

            window.click("board-manifest-edit", cx);
            window.draw(cx).clear(cx);
            // 同一个抽屉换了内容：编辑器顶掉了看板。
            assert!(window.find("board-manifest-editor").visible());
            assert!(window.try_find("board-trigger").is_none());
        })
        .unwrap();

        // 清单文本走延迟更新，跟工作流文件一样要另一帧才到编辑器。
        for _ in 0..2 {
            cx.update_window(handle, |_, window, cx| {
                window.draw(cx).clear(cx);
            })
            .unwrap();
        }

        cx.update_window(handle, |_, window, cx| {
            let text = view.read_with(cx, |view, cx| {
                view.manifest_editor.read(cx).value().to_string()
            });
            assert!(
                text.contains("web-arm"),
                "编辑器没有拿到仓库里的清单：{text:?}"
            );

            // 取消编辑回到看板那一面。
            window.click("board-manifest-cancel", cx);
            window.draw(cx).clear(cx);
            assert!(window.try_find("board-manifest-editor").is_none());
            assert!(window.find("board-trigger").visible());
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn the_runs_table_follows_the_branch_filter(cx: &mut TestAppContext) {
        let (handle, view, _editor) = workspace_page(cx);
        let handle: gpui_kit::AnyWindowHandle = handle.into();

        // Shut, the page shows no drawer, and the header offers to open one.
        cx.update_window(handle, |_, window, cx| {
            window.draw(cx).clear(cx);
            assert!(window.try_find("runs-drawer").is_none());
            assert!(window.try_find("open-runs-drawer").is_some());
            assert_eq!(view.read_with(cx, |view, cx| view.run_rows(cx)), 2);
        })
        .unwrap();

        // Opening through the link would fetch the runs on the tokio runtime,
        // which the GPUI test scheduler refuses to observe; the test flips the
        // view state that the link flips.
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.begin_drawer_open(cx);
            });
            window.draw(cx).clear(cx);

            // The runs list is a drawer over the editor, not a page of its own:
            // the workflow page stays where it is, editor included.
            assert!(window.find("runs-drawer").visible());
            assert!(
                window.try_find("workflow-title-1").is_some(),
                "the workflow page went away with the drawer open"
            );
            assert!(
                window.try_find("workflow-editor-pane").is_some(),
                "the editor was unloaded instead of covered"
            );
            assert!(window.try_find("close-runs-drawer").is_some());

            // Typing a branch into the filter leaves only that branch's runs.
            window.click("runs-branch-filter", cx);
            window.input("main", cx);
        })
        .unwrap();

        // The input reports its change after the keystroke, so the filter lands
        // on the next update.
        cx.update_window(handle, |_, window, cx| {
            window.draw(cx).clear(cx);
            assert_eq!(window.find("runs-branch-filter").value(), Some("main"));
            assert_eq!(view.read_with(cx, |view, cx| view.run_rows(cx)), 1);

            // Closing plays the slide out — the drawer is still in the tree,
            // and only leaves when the slide is done.
            view.update(cx, |view, cx| {
                view.begin_drawer_close(cx);
            });
            window.draw(cx).clear(cx);
            assert!(
                window.try_find("runs-drawer").is_some(),
                "the drawer vanished instead of sliding out"
            );
        })
        .unwrap();

        // The slide finishes on its own, and the drawer leaves with it.
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(300));
        cx.run_until_parked();

        cx.update_window(handle, |_, window, cx| {
            window.draw(cx).clear(cx);
            assert!(window.try_find("runs-drawer").is_none());
            assert!(window.try_find("open-runs-drawer").is_some());
        })
        .unwrap();
    }
}
