use super::AppView;
use super::*;

impl AppView {
    pub(super) fn choose_repository(&mut self, full_name: String, cx: &mut Context<Self>) {
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let picker = self.picker.clone();
        let workspace = self.workspace.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let picker = picker.clone();
                let full_name = full_name.clone();
                async move {
                    picker.lock().await.select(&full_name).await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            let task = runtime.spawn({
                let workspace = workspace.clone();
                let full_name = full_name.clone();
                async move {
                    workspace.lock().await.enter(&full_name);
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            Self::load_workspace(&gateway, &manager, &workspace, &runtime, &this, cx).await;
        })
        .detach();
    }
    pub(super) fn leave_workspace(&mut self, cx: &mut Context<Self>) {
        let picker = self.picker.clone();
        let workspace = self.workspace.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let picker = picker.clone();
                async move {
                    picker.lock().await.leave_workspace().await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            let task = runtime.spawn({
                let workspace = workspace.clone();
                async move {
                    workspace.lock().await.leave();
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            if let Err(error) = this.update(cx, |this, cx| {
                this.selected = None;
                this.workflows.clear();
                this.workflows_state = LoadState::Idle;
                this.runs.clear();
                this.runs_state = LoadState::Idle;
                this.runs_has_more = false;
                this.runs_workflow_filter = None;
                this.workspace_tab = WorkspaceTab::Workflows;
                cx.notify();
            }) {
                warn!(?error, "the view was gone before the update landed");
            };
        })
        .detach();
    }
    pub(super) fn load_more(&mut self, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
        let picker = self.picker.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };

            let task = runtime.spawn({
                let picker = picker.clone();
                async move {
                    picker.lock().await.load_more(&token).await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            Self::refresh_picker(&picker, &this, cx).await;
        })
        .detach();
    }
    pub(super) fn change_sort(&mut self, sort: RepositorySort, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
        let picker = self.picker.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };

            let task = runtime.spawn({
                let picker = picker.clone();
                async move {
                    let mut guard = picker.lock().await;
                    guard.set_sort(sort);
                    guard.reload(&token).await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            Self::refresh_picker(&picker, &this, cx).await;
        })
        .detach();
    }
    pub(super) fn repository_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let query = self.search_input.read(cx).value().to_string();
        let visible = filter_repositories(&self.repos, &query);

        let items = visible
            .iter()
            .map(|repository| {
                let full_name = repository.full_name.clone();
                let visibility = if repository.is_private {
                    labels::REPOSITORIES_PRIVATE
                } else {
                    labels::REPOSITORIES_PUBLIC
                };
                Button::new(SharedString::from(format!("repo-{}", repository.full_name)))
                    .label(format!("{}（{}）", repository.full_name, visibility))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.choose_repository(full_name.clone(), cx)
                    }))
            })
            .collect::<Vec<_>>();

        let mut list = div().flex().flex_col().gap_1().children(items);

        match self.repo_state {
            RepositoryListState::Loading if self.repos.is_empty() => {
                list = list.child(pickable(labels::REPOSITORIES_LOADING));
            }
            RepositoryListState::Failed(problem) => {
                list = list.child(Label::new(problem_text(problem)).text_sm());
            }
            RepositoryListState::Loaded if visible.is_empty() => {
                list = list.child(pickable(labels::REPOSITORIES_EMPTY));
            }
            _ => {}
        }

        if self.repo_has_more {
            list = list.child(
                Button::new("load-more")
                    .label(labels::REPOSITORIES_LOAD_MORE)
                    .on_click(cx.listener(|this, _, _, cx| this.load_more(cx))),
            );
        }

        let updated_selected = self.repo_sort == RepositorySort::Updated;
        let mut sort_updated = Button::new("sort-updated").label(labels::REPOSITORIES_SORT_UPDATED);
        if updated_selected {
            sort_updated = sort_updated.primary();
        }
        let mut sort_pushed = Button::new("sort-pushed").label(labels::REPOSITORIES_SORT_PUSHED);
        if !updated_selected {
            sort_pushed = sort_pushed.primary();
        }

        let sort_row = div()
            .flex()
            .flex_row()
            .gap_2()
            .child(sort_updated.on_click(
                cx.listener(|this, _, _, cx| this.change_sort(RepositorySort::Updated, cx)),
            ))
            .child(sort_pushed.on_click(
                cx.listener(|this, _, _, cx| this.change_sort(RepositorySort::Pushed, cx)),
            ));

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .child(pickable(labels::REPOSITORIES_TITLE))
            .child(Input::new(&self.search_input))
            .child(sort_row)
            .child(list)
            .into_any_element()
    }
}
