use super::*;
use super::{AppView, RUN_POLL_SECONDS};

impl AppView {
    pub(super) async fn refresh_workspace(
        workspace: &Arc<Mutex<Workspace>>,
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
        let runs = guard.runs().to_vec();
        let runs_state = guard.runs_state();
        let runs_has_more = guard.runs_has_more();
        let selected = guard.repository().map(str::to_owned);
        let status = guard.run_filter().status;
        let workflow_filter = guard.run_filter().workflow_id;
        drop(guard);

        if let Err(error) = this.update(cx, |this, cx| {
            this.workspace_tab = tab;
            this.workflows = workflows;
            this.workflows_state = workflows_state;
            this.selected_workflow_id = selected_workflow;
            this.workflow_file = workflow_file;
            this.workflow_file_state = workflow_file_state;
            this.runs = runs;
            this.runs_state = runs_state;
            this.runs_has_more = runs_has_more;
            this.selected = selected;
            this.runs_status_filter = status;
            this.runs_workflow_filter = workflow_filter;
            this.poll_runs_if_needed(cx);
            cx.notify();
        }) {
            warn!(?error, "the view was gone before the update landed");
        };
    }
    pub(super) async fn load_workspace(
        gateway: &Arc<dyn GitHubGateway>,
        manager: &Arc<Mutex<AuthManager>>,
        workspace: &Arc<Mutex<Workspace>>,
        runtime: &TokioRuntime,
        this: &WeakEntity<AppView>,
        cx: &mut AsyncApp,
    ) {
        let token = { manager.lock().await.token() };
        let Some(token) = token else {
            return;
        };

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

        Self::refresh_workspace(workspace, this, cx).await;
    }
    pub(super) fn set_workspace_tab(&mut self, tab: WorkspaceTab, cx: &mut Context<Self>) {
        self.workspace_tab = tab;
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let workspace = self.workspace.clone();
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

            Self::refresh_workspace(&workspace, &this, cx).await;
        })
        .detach();
    }
    pub(super) fn load_more_runs(&mut self, cx: &mut Context<Self>) {
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let workspace = self.workspace.clone();
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
            Self::refresh_workspace(&workspace, &this, cx).await;
        })
        .detach();
    }
    pub(super) fn set_workflow_filter(&mut self, workflow_id: Option<u64>, cx: &mut Context<Self>) {
        self.runs_workflow_filter = workflow_id;
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let workspace = self.workspace.clone();
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
                    let mut guard = workspace.lock().await;
                    guard.set_workflow_filter(workflow_id);
                    guard.reload_runs(&*gateway, &token).await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
            Self::refresh_workspace(&workspace, &this, cx).await;
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

                Self::refresh_workspace(&workspace, &this, cx).await;
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
    pub(super) fn workspace_shell(&self, cx: &mut Context<Self>) -> AnyElement {
        let content = match self.workspace_tab {
            WorkspaceTab::Workflows => self.workflows_panel_ui(cx),
            WorkspaceTab::Runs => self.runs_panel_ui(cx),
        };

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .size_full()
            .child(self.workspace_header_ui(cx))
            .child(content)
            .into_any_element()
    }
    /// The page title, and the way across to the other view. Each view is a
    /// page with a heading rather than one more row of buttons.
    fn workspace_header_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        let (title, back) = match self.workspace_tab {
            WorkspaceTab::Workflows => (labels::WORKSPACE_WORKFLOWS, None),
            WorkspaceTab::Runs => (
                labels::WORKSPACE_RUNS,
                Some(
                    Button::new("back-to-workflows")
                        .label(labels::WORKSPACE_BACK_ARROW)
                        .tooltip(labels::WORKSPACE_BACK_TO_WORKFLOWS)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_workspace_tab(WorkspaceTab::Workflows, cx);
                        }))
                        .into_any_element(),
                ),
            ),
        };

        let heading = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .children(back)
            .child(Label::new(title).text_lg());

        // The workflow page's one action: start a new workflow file.
        let new_workflow = (self.workspace_tab == WorkspaceTab::Workflows).then(|| {
            let (id, label) = if self.creating_workflow {
                ("new-workflow-off", labels::WORKFLOW_NEW_CANCEL)
            } else {
                ("new-workflow-on", labels::WORKFLOW_NEW)
            };
            Button::new(id)
                .label(label)
                .on_click(cx.listener(|this, _, window, cx| this.toggle_new_workflow(window, cx)))
                .into_any_element()
        });
        let heading = heading.children(new_workflow);

        let mut header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(heading);

        match self.workspace_tab {
            WorkspaceTab::Workflows => {
                let link_color = cx.theme().link;
                header = header.child(
                    div()
                        .id("open-runs")
                        .text_color(link_color)
                        .cursor_pointer()
                        .child(labels::WORKSPACE_RUN_HISTORY)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_workspace_tab(WorkspaceTab::Runs, cx);
                        })),
                );
            }
            WorkspaceTab::Runs => {}
        }

        header.into_any_element()
    }
    /// Show a workflow's file. Selecting is cheap; the file itself is fetched
    /// in the background afterwards.
    pub(super) fn select_workflow(&mut self, workflow_id: u64, cx: &mut Context<Self>) {
        // Browsing away from the form means leaving it.
        self.creating_workflow = false;
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let workspace = self.workspace.clone();
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
            Self::refresh_workspace(&workspace, &this, cx).await;

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
            Self::refresh_workspace(&workspace, &this, cx).await;
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
        if self.yaml_editor_text.as_deref() == Some(contents.as_str()) {
            return;
        }
        self.yaml_editor_text = Some(contents.clone());

        let editor = self.yaml_editor.clone();
        window.defer(cx, move |window, cx| {
            editor.update(cx, |state, cx| {
                if state.value().as_ref() != contents.as_str() {
                    state.set_value(contents, window, cx);
                }
            });
        });
    }
    /// Trigger the selected workflow on the default branch, then pull the runs
    /// so the new run shows up without a manual refresh.
    pub(super) fn run_selected_workflow(&mut self, cx: &mut Context<Self>) {
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
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
            Self::refresh_workspace(&workspace, &this, cx).await;
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
        cx.notify();
    }

    /// A blank form: no file name yet, the first runner, no container, one job.
    fn reset_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.draft_file_name
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.draft_name
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.draft_container
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.draft_runner = RUNNERS[0].to_owned();

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
        WorkflowDraft {
            file_name: self.draft_file_name.read(cx).value().to_string(),
            name: self.draft_name.read(cx).value().to_string(),
            runs_on: self.draft_runner.clone(),
            container: Some(self.draft_container.read(cx).value().to_string()),
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

    /// Turn the form into a workflow file, push it, and show it in the editor.
    pub(super) fn create_workflow(&mut self, cx: &mut Context<Self>) {
        let draft = self.draft_from_form(cx);
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
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
                        Ok(()) => Notice {
                            kind: NoticeKind::Info,
                            text: labels::WORKFLOW_NEW_PUSHED.to_owned(),
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
                        Err(CreateProblem::AlreadyExists) => Notice {
                            kind: NoticeKind::Warning,
                            text: labels::WORKFLOW_NEW_EXISTS.to_owned(),
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
            Self::refresh_workspace(&workspace, &this, cx).await;

            if created
                && let Err(error) = this.update(cx, |this, cx| {
                    this.creating_workflow = false;
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
            Self::refresh_workspace(&workspace, &this, cx).await;
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
            .child(if self.creating_workflow {
                self.workflow_draft_ui(cx)
            } else {
                self.workflow_file_ui(cx)
            })
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
            .child(self.draft_runner_row(cx))
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
    fn draft_runner_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut row = div().flex().flex_row().gap_2();
        for runner in RUNNERS {
            let mut button = Button::new(SharedString::from(format!("runner-{runner}")));
            button = button.label(runner);
            if self.draft_runner == runner {
                button = button.primary();
            }
            row = row.child(button.on_click(cx.listener(move |this, _, _, cx| {
                this.draft_runner = runner.to_owned();
                cx.notify();
            })));
        }

        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(Label::new(labels::WORKFLOW_NEW_RUNNER).text_sm())
            .child(row)
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
        let mut submit = div().flex().flex_row().items_center().gap_3().child(
            Button::new("draft-create")
                .label(labels::WORKFLOW_NEW_CREATE)
                .primary()
                .disabled(problem.is_some())
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
    pub(super) fn runs_panel_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.open_run.is_some() {
            return self.run_detail_ui(cx);
        }

        let status_button = |id: &'static str, label: &'static str, active: bool| {
            let mut button = Button::new(id).label(label);
            if active {
                button = button.primary();
            }
            button
        };

        let status_row = div()
            .flex()
            .flex_row()
            .gap_2()
            .child(
                status_button(
                    "runs-status-all",
                    labels::RUNS_FILTER_ALL,
                    self.runs_status_filter == RunStatusFilter::All,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.runs_status_filter = RunStatusFilter::All;
                    cx.notify();
                })),
            )
            .child(
                status_button(
                    "runs-status-running",
                    labels::RUNS_FILTER_RUNNING,
                    self.runs_status_filter == RunStatusFilter::Running,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.runs_status_filter = RunStatusFilter::Running;
                    cx.notify();
                })),
            )
            .child(
                status_button(
                    "runs-status-completed",
                    labels::RUNS_FILTER_COMPLETED,
                    self.runs_status_filter == RunStatusFilter::Completed,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.runs_status_filter = RunStatusFilter::Completed;
                    cx.notify();
                })),
            );

        let mut workflow_buttons = vec![
            status_button(
                "workflow-all",
                labels::RUNS_FILTER_WORKFLOW_ALL,
                self.runs_workflow_filter.is_none(),
            )
            .on_click(cx.listener(|this, _, _, cx| this.set_workflow_filter(None, cx)))
            .into_any_element(),
        ];
        for workflow in &self.workflows {
            let id = workflow.id;
            let mut button = Button::new(SharedString::from(format!("workflow-{id}")))
                .label(workflow.name.clone());
            if self.runs_workflow_filter == Some(id) {
                button = button.primary();
            }
            workflow_buttons.push(
                button
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.set_workflow_filter(Some(id), cx)),
                    )
                    .into_any_element(),
            );
        }
        let workflow_row = div().flex().flex_row().gap_2().children(workflow_buttons);

        let branch_text = self.branch_input.read(cx).value().to_string();
        let branch_filter = if branch_text.trim().is_empty() {
            None
        } else {
            Some(branch_text.trim().to_owned())
        };
        let filter = RunFilter {
            status: self.runs_status_filter,
            branch: branch_filter,
            workflow_id: self.runs_workflow_filter,
        };
        let visible = filter_runs(&self.runs, &filter);

        let rows = visible
            .iter()
            .map(|run| {
                let run = run.clone();
                Button::new(SharedString::from(format!("run-{}", run.id)))
                    .label(format!(
                        "{}｜{}｜{}｜{}｜{}｜{}｜{}",
                        run.status.label(),
                        run.name,
                        run.conclusion
                            .clone()
                            .unwrap_or_else(|| labels::VALUE_MISSING.to_owned()),
                        run.branch
                            .clone()
                            .unwrap_or_else(|| labels::VALUE_MISSING.to_owned()),
                        run.event,
                        run.actor
                            .clone()
                            .unwrap_or_else(|| labels::VALUE_MISSING.to_owned()),
                        run.created_at
                            .clone()
                            .unwrap_or_else(|| labels::VALUE_MISSING.to_owned()),
                    ))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.open_run_detail(run.clone(), cx)),
                    )
            })
            .collect::<Vec<_>>();

        let mut list = div().flex().flex_col().gap_1().children(rows);
        match self.runs_state {
            LoadState::Loading if self.runs.is_empty() => {
                list = list.child(pickable(labels::RUNS_LOADING));
            }
            LoadState::Failed(problem) => {
                list = list.child(Label::new(problem_text(problem)).text_sm());
            }
            LoadState::Loaded if visible.is_empty() => {
                list = list.child(pickable(labels::RUNS_EMPTY));
            }
            _ => {}
        }
        if self.runs_has_more {
            list = list.child(
                Button::new("runs-load-more")
                    .label(labels::RUNS_LOAD_MORE)
                    .on_click(cx.listener(|this, _, _, cx| this.load_more_runs(cx))),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(status_row)
            .child(workflow_row)
            .child(Input::new(&self.branch_input))
            .child(list)
            .into_any_element()
    }
}

/// What to tell the user about a form that is not a workflow yet.
fn draft_problem_text(problem: DraftProblem) -> &'static str {
    match problem {
        DraftProblem::FileName => labels::WORKFLOW_NEW_BAD_FILE,
        DraftProblem::JobId => labels::WORKFLOW_NEW_BAD_JOB,
        DraftProblem::NoJobs => labels::WORKFLOW_NEW_NO_JOBS,
    }
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
    use crate::github::{Account, GitHubGateway, Workflow};
    use crate::runtime::TokioRuntime;
    use crate::store::Store;

    use super::{AppView, Root, Services};

    /// A signed-in view sitting on an open repository with one workflow, which
    /// is the state the workflow page is designed for.
    fn workspace_page(
        cx: &mut TestAppContext,
    ) -> (WindowHandle<Root>, Entity<AppView>, Entity<EditorState>) {
        cx.update(gpui_kit::init);

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
        let (handle, _view, _editor) = workspace_page(cx);
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

            // Jobs can be added to the draft while it is open.
            window.click("draft-job-add", cx);
            window.draw(cx).clear(cx);
            assert!(
                window.try_find("draft-job-remove-1").is_some(),
                "the second job row never appeared"
            );

            window.click("new-workflow-off", cx);
            window.draw(cx).clear(cx);
            assert!(window.try_find("workflow-draft-pane").is_none());
        })
        .unwrap();
    }
}
