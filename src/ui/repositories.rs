use super::AppView;

use gpui_kit::component::Sizable as _;
use gpui_kit::component::table::{DataTable, TableDelegate, TableState};

use super::*;

const REPO_COLUMN_NAME: &str = "full_name";
const REPO_COLUMN_VISIBILITY: &str = "visibility";
const REPO_COLUMN_BRANCH: &str = "default_branch";
const REPO_COLUMN_COMMIT: &str = "commit";
const REPO_COLUMN_COMMIT_DATE: &str = "commit_date";

/// The repository picker as a `DataTable`. Rows are whatever the picker
/// currently shows: the loaded repositories after the name filter.
pub(super) struct RepositoryTableDelegate {
    view: WeakEntity<AppView>,
    repositories: Vec<Repository>,
    columns: Vec<Column>,
}

impl RepositoryTableDelegate {
    pub(super) fn new(view: WeakEntity<AppView>) -> Self {
        Self {
            view,
            repositories: Vec::new(),
            columns: vec![
                Column::new(REPO_COLUMN_NAME, labels::REPOSITORIES_COLUMN_NAME).width(280.),
                Column::new(
                    REPO_COLUMN_VISIBILITY,
                    labels::REPOSITORIES_COLUMN_VISIBILITY,
                )
                .width(90.),
                Column::new(REPO_COLUMN_BRANCH, labels::REPOSITORIES_COLUMN_BRANCH).width(150.),
                Column::new(REPO_COLUMN_COMMIT, labels::REPOSITORIES_COLUMN_COMMIT).width(360.),
                Column::new(
                    REPO_COLUMN_COMMIT_DATE,
                    labels::REPOSITORIES_COLUMN_COMMIT_DATE,
                )
                .width(170.),
            ],
        }
    }

    fn set_repositories(&mut self, repositories: Vec<Repository>) {
        self.repositories = repositories;
    }
}

impl TableDelegate for RepositoryTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.repositories.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(repository) = self.repositories.get(row_ix) else {
            return div().into_any_element();
        };

        match self.columns[col_ix].key.as_ref() {
            REPO_COLUMN_NAME => {
                let full_name = repository.full_name.clone();
                let view = self.view.clone();
                Button::new(SharedString::from(format!("repo-{full_name}")))
                    .ghost()
                    .xsmall()
                    .label(full_name.clone())
                    .on_click(move |_, _, cx| {
                        let full_name = full_name.clone();
                        let _ = view.update(cx, |this, cx| this.choose_repository(full_name, cx));
                    })
                    .into_any_element()
            }
            REPO_COLUMN_VISIBILITY => {
                let visibility = if repository.is_private {
                    labels::REPOSITORIES_PRIVATE
                } else {
                    labels::REPOSITORIES_PUBLIC
                };
                Label::new(visibility).text_sm().into_any_element()
            }
            REPO_COLUMN_BRANCH => Label::new(
                repository
                    .default_branch
                    .clone()
                    .unwrap_or_else(|| labels::VALUE_MISSING.to_owned()),
            )
            .text_sm()
            .into_any_element(),
            REPO_COLUMN_COMMIT => Label::new(commit_message(repository.latest_commit.as_ref()))
                .text_sm()
                .into_any_element(),
            REPO_COLUMN_COMMIT_DATE => Label::new(commit_date(
                repository
                    .latest_commit
                    .as_ref()
                    .and_then(|commit| commit.committed_at.as_deref()),
            ))
            .text_sm()
            .into_any_element(),
            _ => div().into_any_element(),
        }
    }
}

/// The first line of the commit message, which is what a table row can show.
fn commit_message(commit: Option<&CommitSummary>) -> String {
    match commit {
        Some(commit) => commit.message.lines().next().unwrap_or("").to_owned(),
        None => labels::VALUE_MISSING.to_owned(),
    }
}

fn commit_date(committed_at: Option<&str>) -> String {
    match committed_at {
        Some(raw) => chrono::DateTime::parse_from_rfc3339(raw)
            .map(|date| date.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|_| raw.to_owned()),
        None => labels::VALUE_MISSING.to_owned(),
    }
}

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
    /// Recompute the table's rows from the loaded repositories and the current
    /// name filter. Called whenever either changes.
    pub(super) fn refresh_repo_table(&mut self, cx: &mut Context<Self>) {
        let query = self.search_input.read(cx).value().to_string();
        let visible = filter_repositories(&self.repos, &query);
        self.repo_table.update(cx, |table, cx| {
            table.delegate_mut().set_repositories(visible);
            table.refresh(cx);
        });
    }
    pub(super) fn repository_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.repo_table.read(cx).delegate().repositories.len();

        let mut list = div().flex().flex_col().gap_1().flex_1().min_h_0();

        if rows > 0 {
            list = list.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(DataTable::new(&self.repo_table).stripe(true)),
            );
        } else {
            let message = match self.repo_state {
                RepositoryListState::Loading => Some(labels::REPOSITORIES_LOADING),
                RepositoryListState::Failed(problem) => Some(problem_text(problem)),
                RepositoryListState::Loaded => Some(labels::REPOSITORIES_EMPTY),
                RepositoryListState::Idle => None,
            };
            if let Some(message) = message {
                list = list.child(pickable(message));
            }
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
            .size_full()
            .child(pickable(labels::REPOSITORIES_TITLE))
            .child(Input::new(&self.search_input))
            .child(sort_row)
            .child(list)
            .into_any_element()
    }
}
