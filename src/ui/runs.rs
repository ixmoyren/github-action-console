//! The run list as a data table: what the Actions tab is for.

use gpui_kit::component::Sizable as _;
use gpui_kit::component::table::{ColumnSort, DataTable, TableDelegate, TableState};

use super::*;

const RUN_COLUMN_STATUS: &str = "status";
const RUN_COLUMN_WORKFLOW: &str = "workflow";
const RUN_COLUMN_CONCLUSION: &str = "conclusion";
const RUN_COLUMN_BRANCH: &str = "branch";
const RUN_COLUMN_EVENT: &str = "event";
const RUN_COLUMN_CREATED: &str = "created";

/// The runs table. Rows are the runs the workspace holds after the branch
/// filter, so the delegate never fetches anything itself.
pub(super) struct RunTableDelegate {
    view: WeakEntity<AppView>,
    runs: Vec<WorkflowRun>,
    /// The repository's workflows, for naming the workflow a run belongs to.
    workflows: Vec<Workflow>,
    columns: Vec<Column>,
}

impl RunTableDelegate {
    pub(super) fn new(view: WeakEntity<AppView>) -> Self {
        Self {
            view,
            runs: Vec::new(),
            workflows: Vec::new(),
            columns: vec![
                Column::new(RUN_COLUMN_STATUS, labels::RUNS_COLUMN_STATUS)
                    .width(110.)
                    .sortable(),
                Column::new(RUN_COLUMN_WORKFLOW, labels::RUNS_COLUMN_WORKFLOW)
                    .width(260.)
                    .sortable(),
                Column::new(RUN_COLUMN_CONCLUSION, labels::RUNS_COLUMN_CONCLUSION)
                    .width(110.)
                    .sortable(),
                Column::new(RUN_COLUMN_BRANCH, labels::RUNS_COLUMN_BRANCH)
                    .width(180.)
                    .sortable(),
                Column::new(RUN_COLUMN_EVENT, labels::RUNS_COLUMN_EVENT)
                    .width(160.)
                    .sortable(),
                Column::new(RUN_COLUMN_CREATED, labels::RUNS_COLUMN_CREATED)
                    .width(180.)
                    .sortable(),
            ],
        }
    }

    fn set_rows(&mut self, runs: Vec<WorkflowRun>, workflows: Vec<Workflow>) {
        self.runs = runs;
        self.workflows = workflows;
    }

    fn rows(&self) -> usize {
        self.runs.len()
    }
}

impl TableDelegate for RunTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.runs.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) {
        let Some(key) = self.columns.get(col_ix).map(|column| column.key.clone()) else {
            return;
        };
        let workflows = self.workflows.clone();
        self.runs.sort_by(|a, b| {
            let order =
                sort_key(a, key.as_ref(), &workflows).cmp(&sort_key(b, key.as_ref(), &workflows));
            match sort {
                ColumnSort::Descending => order.reverse(),
                _ => order,
            }
        });
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(run) = self.runs.get(row_ix) else {
            return div().into_any_element();
        };

        match self.columns[col_ix].key.as_ref() {
            // The workflow's name is what opens the run, the way a repository's
            // name opens the repository.
            RUN_COLUMN_WORKFLOW => {
                let name = workflow_name(run, &self.workflows);
                let run = run.clone();
                let view = self.view.clone();
                Button::new(SharedString::from(format!("run-{}", run.id)))
                    .ghost()
                    .xsmall()
                    .label(name)
                    .on_click(move |_, _, cx| {
                        let run = run.clone();
                        let _ = view.update(cx, |this, cx| this.open_run_detail(run, cx));
                    })
                    .into_any_element()
            }
            RUN_COLUMN_STATUS => Label::new(run.status.label()).text_sm().into_any_element(),
            RUN_COLUMN_CONCLUSION => {
                Label::new(labels::conclusion_label(run.conclusion.as_deref()))
                    .text_sm()
                    .into_any_element()
            }
            RUN_COLUMN_BRANCH => Label::new(missing(&run.branch))
                .text_sm()
                .into_any_element(),
            RUN_COLUMN_EVENT => Label::new(run.event.clone()).text_sm().into_any_element(),
            RUN_COLUMN_CREATED => Label::new(created_at(run)).text_sm().into_any_element(),
            _ => div().into_any_element(),
        }
    }
}

/// The name of the workflow a run belongs to. The run's own name stands in for
/// a workflow the listing did not return.
fn workflow_name(run: &WorkflowRun, workflows: &[Workflow]) -> String {
    workflows
        .iter()
        .find(|workflow| workflow.id == run.workflow_id)
        .map(|workflow| workflow.name.clone())
        .unwrap_or_else(|| run.name.clone())
}

/// The text a column sorts by. A missing value sorts as an empty string.
fn sort_key(run: &WorkflowRun, column: &str, workflows: &[Workflow]) -> String {
    match column {
        RUN_COLUMN_STATUS => run.status.label().to_owned(),
        RUN_COLUMN_WORKFLOW => workflow_name(run, workflows).to_lowercase(),
        RUN_COLUMN_CONCLUSION => labels::conclusion_label(run.conclusion.as_deref()),
        RUN_COLUMN_BRANCH => run.branch.clone().unwrap_or_default(),
        RUN_COLUMN_EVENT => run.event.clone(),
        // GitHub writes ISO-8601, so text order is time order.
        RUN_COLUMN_CREATED => run.created_at.clone().unwrap_or_default(),
        _ => String::new(),
    }
}

fn missing(value: &Option<String>) -> String {
    value
        .clone()
        .unwrap_or_else(|| labels::VALUE_MISSING.to_owned())
}

fn created_at(run: &WorkflowRun) -> String {
    match run.created_at.as_deref() {
        Some(created_at) => super::repositories::format_commit_date(created_at),
        None => labels::VALUE_MISSING.to_owned(),
    }
}

impl AppView {
    /// Recompute the table's rows from the loaded runs and the current branch
    /// filter. Called whenever either changes.
    pub(super) fn refresh_run_table(&mut self, cx: &mut Context<Self>) {
        let branch = self.branch_input.read(cx).value().trim().to_owned();
        let filter = RunFilter {
            branch: (!branch.is_empty()).then_some(branch),
            ..RunFilter::default()
        };
        let visible = filter_runs(&self.runs, &filter);
        let workflows = self.workflows.clone();
        self.run_table.update(cx, |table, cx| {
            table.delegate_mut().set_rows(visible, workflows);
            table.refresh(cx);
        });
    }

    pub(super) fn run_rows(&self, cx: &App) -> usize {
        self.run_table.read(cx).delegate().rows()
    }

    pub(super) fn run_table_ui(&self) -> AnyElement {
        DataTable::new(&self.run_table)
            .stripe(true)
            .into_any_element()
    }
}
