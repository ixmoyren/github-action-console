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
    pub(super) fn workspace_shell(&self, full_name: &str, cx: &mut Context<Self>) -> AnyElement {
        let mut workflows_tab = Button::new("tab-workflows").label(labels::WORKSPACE_WORKFLOWS);
        if self.workspace_tab == WorkspaceTab::Workflows {
            workflows_tab = workflows_tab.primary();
        }
        let mut runs_tab = Button::new("tab-runs").label(labels::WORKSPACE_RUNS);
        if self.workspace_tab == WorkspaceTab::Runs {
            runs_tab = runs_tab.primary();
        }

        let tabs = div()
            .flex()
            .flex_row()
            .gap_2()
            .child(workflows_tab.on_click(cx.listener(|this, _, _, cx| {
                this.set_workspace_tab(WorkspaceTab::Workflows, cx);
            })))
            .child(runs_tab.on_click(cx.listener(|this, _, _, cx| {
                this.set_workspace_tab(WorkspaceTab::Runs, cx);
            })));

        let content = match self.workspace_tab {
            WorkspaceTab::Workflows => self.workflows_panel_ui(),
            WorkspaceTab::Runs => self.runs_panel_ui(cx),
        };

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .child(
                Button::new("back-to-repos")
                    .label(labels::WORKSPACE_BACK)
                    .on_click(cx.listener(|this, _, _, cx| this.leave_workspace(cx))),
            )
            .child(pickable(full_name.to_owned()))
            .child(tabs)
            .child(content)
            .into_any_element()
    }
    pub(super) fn workflows_panel_ui(&self) -> AnyElement {
        let mut panel = div().flex().flex_col().gap_2().children(
            self.workflows
                .iter()
                .map(|workflow| pickable(workflow.name.clone())),
        );

        match self.workflows_state {
            LoadState::Loading => panel = panel.child(pickable(labels::WORKFLOWS_LOADING)),
            LoadState::Failed(problem) => {
                panel = panel.child(Label::new(problem_text(problem)).text_sm())
            }
            LoadState::Loaded if self.workflows.is_empty() => {
                panel = panel.child(pickable(labels::WORKFLOWS_EMPTY))
            }
            _ => {}
        }

        panel.into_any_element()
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
