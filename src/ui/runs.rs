//! The run list: what the Actions tab is for.
//!
//! 这张表不是 `DataTable`：跑完的那一行要在**这一行下面、表格里面**展开它的构建产物，
//! 而 `DataTable` 的行高由它内部的虚拟列表统一给定（`uniform_list`），行不会长高。
//! 所以表头、列宽、排序、展开都由这里自己做，样子照着原来那张表格来。

use gpui_kit::component::Sizable as _;

use super::*;

const RUN_COLUMN_STATUS: &str = "status";
const RUN_COLUMN_WORKFLOW: &str = "workflow";
const RUN_COLUMN_CONCLUSION: &str = "conclusion";
const RUN_COLUMN_BRANCH: &str = "branch";
const RUN_COLUMN_EVENT: &str = "event";
const RUN_COLUMN_CREATED: &str = "created";
const RUN_COLUMN_ACTION: &str = "action";

/// 一条运行那一行的高度。展开的详情自己占自己的高度：它接在行下面，不在行里。
const RUN_ROW_HEIGHT: f32 = 40.0;

/// 一列：排序用的键、表头文字、宽度。
struct RunColumn {
    key: &'static str,
    name: &'static str,
    /// `None` 的那一列吃掉剩下的宽度。
    width: Option<f32>,
}

/// 表头有哪些列，表的每一行就照着它排。
const RUN_COLUMNS: [RunColumn; 7] = [
    RunColumn {
        key: RUN_COLUMN_STATUS,
        name: labels::RUNS_COLUMN_STATUS,
        width: Some(110.),
    },
    RunColumn {
        key: RUN_COLUMN_WORKFLOW,
        name: labels::RUNS_COLUMN_WORKFLOW,
        width: None,
    },
    RunColumn {
        key: RUN_COLUMN_CONCLUSION,
        name: labels::RUNS_COLUMN_CONCLUSION,
        width: Some(100.),
    },
    RunColumn {
        key: RUN_COLUMN_BRANCH,
        name: labels::RUNS_COLUMN_BRANCH,
        width: Some(160.),
    },
    RunColumn {
        key: RUN_COLUMN_EVENT,
        name: labels::RUNS_COLUMN_EVENT,
        width: Some(130.),
    },
    RunColumn {
        key: RUN_COLUMN_CREATED,
        name: labels::RUNS_COLUMN_CREATED,
        width: Some(170.),
    },
    // 操作列不参与排序：它是按钮，不是可以比较的值。
    RunColumn {
        key: RUN_COLUMN_ACTION,
        name: labels::RUNS_COLUMN_ACTION,
        width: Some(110.),
    },
];

/// 一列的宽度：固定宽度，或者吃掉剩下的宽度；两边留一点气。
fn run_cell_box(column: &RunColumn) -> Div {
    let cell = match column.width {
        Some(width) => div().w(px(width)).flex_shrink_0(),
        None => div().flex_1().min_w_0(),
    };
    cell.px_2()
}

impl AppView {
    /// Recompute the visible runs from the loaded runs, the branch filter and
    /// the current sort. Called whenever any of them changes.
    pub(super) fn refresh_run_table(&mut self, cx: &mut Context<Self>) {
        let branch = self.branch_input.read(cx).value().trim().to_owned();
        let filter = RunFilter {
            branch: (!branch.is_empty()).then_some(branch),
            ..RunFilter::default()
        };
        let mut visible = filter_runs(&self.runs, &filter);
        if let Some((key, descending)) = self.run_sort {
            let workflows = self.workflows.clone();
            visible.sort_by(|a, b| {
                let order = sort_key(a, key, &workflows).cmp(&sort_key(b, key, &workflows));
                if descending { order.reverse() } else { order }
            });
        }
        self.runs_visible = visible;
        cx.notify();
    }

    pub(super) fn run_rows(&self) -> usize {
        self.runs_visible.len()
    }

    /// 表头加每一行。跑完的那一行展开时，详情就接在它自己下面——还在同一张表里。
    pub(super) fn run_table_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        let rows = self
            .runs_visible
            .iter()
            .map(|run| self.run_entry_ui(run, cx))
            .collect::<Vec<_>>();
        div()
            .flex()
            .flex_col()
            .w_full()
            // 上面那一层是普通块容器，撑满它靠 h_full：flex_1 在这层不起作用，
            // 高度会塌成内容高度，下面那截可滚动区就只剩零高。
            .h_full()
            .min_h_0()
            // 表头留在原地，行多了滚的是下面这一截。
            .child(self.run_header_ui(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .id("runs-scroll")
                    .children(rows),
            )
            .into_any_element()
    }

    /// 表头：点一列按它排，再点一次换方向，第三次回到原来的顺序。
    fn run_header_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut header = div()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .h(px(RUN_ROW_HEIGHT))
            .border_b_1()
            .border_color(cx.theme().table_row_border)
            .bg(cx.theme().tokens.table_even);

        for column in RUN_COLUMNS.iter() {
            let key = column.key;
            let direction = match self.run_sort {
                Some((current, true)) if current == key => " ↓",
                Some((current, false)) if current == key => " ↑",
                _ => "",
            };
            let name = format!("{}{}", column.name, direction);
            let content: AnyElement = if key == RUN_COLUMN_ACTION {
                Label::new(name).text_sm().into_any_element()
            } else {
                div()
                    .id(SharedString::from(format!("run-sort-{key}")))
                    .test_support()
                    .cursor_pointer()
                    .child(Label::new(name).text_sm())
                    .on_click(cx.listener(move |this, _, _, cx| this.sort_runs(key, cx)))
                    .into_any_element()
            };
            header = header.child(run_cell_box(column).child(content));
        }
        header.into_any_element()
    }

    fn sort_runs(&mut self, key: &'static str, cx: &mut Context<Self>) {
        self.run_sort = match self.run_sort {
            Some((current, false)) if current == key => Some((key, true)),
            Some((current, true)) if current == key => None,
            _ => Some((key, false)),
        };
        self.refresh_run_table(cx);
    }

    /// 一条运行：它自己那一行，加上（展开时）接在下面的详情。
    fn run_entry_ui(&self, run: &WorkflowRun, cx: &mut Context<Self>) -> AnyElement {
        let mut entry = div()
            .flex()
            .flex_col()
            .w_full()
            .border_b_1()
            .border_color(cx.theme().table_row_border)
            .child(self.run_row_ui(run, cx));
        if self.expanded_run == Some(run.id) {
            entry = entry.child(self.run_artifacts_ui(run, cx));
        }
        entry.into_any_element()
    }

    fn run_row_ui(&self, run: &WorkflowRun, cx: &mut Context<Self>) -> AnyElement {
        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .h(px(RUN_ROW_HEIGHT))
            .hover(|this| this.bg(cx.theme().tokens.table_hover));
        for column in RUN_COLUMNS.iter() {
            row = row.child(run_cell_box(column).child(self.run_cell_ui(run, column.key, cx)));
        }
        row.into_any_element()
    }

    fn run_cell_ui(&self, run: &WorkflowRun, key: &str, cx: &mut Context<Self>) -> AnyElement {
        match key {
            // The workflow's name is what opens the run, the way a repository's
            // name opens the repository.
            RUN_COLUMN_WORKFLOW => {
                let name = workflow_name(run, &self.workflows);
                let run = run.clone();
                let view = cx.weak_entity();
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
            // 「已完成」是这一行的入口：点它在这行下面展开构建产物——不是弹一个框，
            // 详情就在表格里。还没跑完的行只是一句状态。
            RUN_COLUMN_STATUS => {
                if run.status != RunStatus::Completed {
                    return Label::new(run.status.label()).text_sm().into_any_element();
                }
                let run_id = run.id;
                Button::new(SharedString::from(format!("run-status-{run_id}")))
                    .ghost()
                    .xsmall()
                    .label(run.status.label())
                    .tooltip(labels::RUNS_BUILD_HINT)
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_run_build(run_id, cx)))
                    .into_any_element()
            }
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
            // 还没跑完的可以取消，跑完了的可以删除——GitHub 的两个动作都落在运行上，
            // 进行中的 job 跟着这次运行一起停或一起没。
            RUN_COLUMN_ACTION => {
                let run_id = run.id;
                let completed = run.status == RunStatus::Completed;
                let (id, label, tooltip, action) = if completed {
                    (
                        format!("run-delete-{run_id}"),
                        labels::RUNS_DELETE,
                        labels::RUNS_DELETE_TOOLTIP,
                        ActionKey::DeleteRun,
                    )
                } else {
                    (
                        format!("run-cancel-{run_id}"),
                        labels::RUNS_CANCEL,
                        labels::RUNS_CANCEL_TOOLTIP,
                        ActionKey::CancelRun,
                    )
                };
                // 上一下还没回来就置灰：同一个动作不能同时来两下。
                let busy = self.action_in_flight(action);
                Button::new(SharedString::from(id))
                    .xsmall()
                    .label(label)
                    .tooltip(tooltip)
                    .disabled(busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if completed {
                            this.delete_run(run_id, cx);
                        } else {
                            this.cancel_run(run_id, cx);
                        }
                    }))
                    .into_any_element()
            }
            _ => div().into_any_element(),
        }
    }

    /// 展开的详情：这条运行产出的构建产物，就在这一行下面、还在表格里。
    fn run_artifacts_ui(&self, run: &WorkflowRun, cx: &mut Context<Self>) -> AnyElement {
        let mut detail = div()
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .px_4()
            .py_3()
            .bg(cx.theme().tokens.table_even)
            .child(Label::new(labels::ARTIFACTS_TITLE).text_sm());

        match self.run_artifacts.get(&run.id) {
            Some(artifacts) if artifacts.state == LoadState::Loaded => {
                if artifacts.artifacts.is_empty() {
                    detail = detail.child(Label::new(labels::RUNS_BUILD_EMPTY).text_sm());
                }
                for artifact in &artifacts.artifacts {
                    detail = detail.child(self.artifact_row(artifact, cx));
                }
                if let Some(text) = download_status(&self.download_state) {
                    detail = detail.child(Label::new(text).text_sm());
                }
            }
            Some(artifacts) if matches!(artifacts.state, LoadState::Failed(_)) => {
                detail = detail.child(Label::new(labels::RUNS_BUILD_FAILED).text_sm());
            }
            _ => {
                detail = detail.child(Label::new(labels::RUNS_BUILD_LOADING).text_sm());
            }
        }
        detail.into_any_element()
    }

    /// 展开详情里的一条构建产物：名字、大小、过期标记，加下载入口。
    ///
    /// 大到要确认的产物停在 `Downloads` 里等人点头，所以那一行那时摆的是确认/取消——
    /// 在表格里下不到东西，等于没给入口。
    fn artifact_row(&self, artifact: &BuildArtifact, cx: &mut Context<Self>) -> AnyElement {
        let expired = if artifact.expired {
            format!("（{}）", labels::ARTIFACTS_EXPIRED)
        } else {
            String::new()
        };
        let row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(pickable(format!(
                "{}｜{} B{}",
                artifact.name, artifact.size_in_bytes, expired
            )));

        let waiting_here = match self.download_state {
            DownloadState::NeedsConfirmation(ref pending) => matches!(
                pending.kind,
                DownloadKind::Artifact { artifact_id } if artifact_id == artifact.id
            ),
            _ => false,
        };
        let artifact_id = artifact.id;
        if waiting_here {
            return row
                .child(
                    Button::new(SharedString::from(format!(
                        "run-build-confirm-{artifact_id}"
                    )))
                    .xsmall()
                    .primary()
                    .label(labels::DOWNLOAD_CONFIRM)
                    .on_click(cx.listener(|this, _, _, cx| this.confirm_download(cx))),
                )
                .child(
                    Button::new(SharedString::from(format!(
                        "run-build-cancel-{artifact_id}"
                    )))
                    .xsmall()
                    .label(labels::DOWNLOAD_CANCEL)
                    .on_click(cx.listener(|this, _, _, cx| this.cancel_download(cx))),
                )
                .into_any_element();
        }

        let artifact = artifact.clone();
        row.child(
            Button::new(SharedString::from(format!(
                "run-build-download-{artifact_id}"
            )))
            .xsmall()
            .label(labels::ARTIFACT_DOWNLOAD)
            .on_click(
                cx.listener(move |this, _, _, cx| this.download_artifact(artifact.clone(), cx)),
            ),
        )
        .into_any_element()
    }

    /// 运行列表里点「已完成」：展开这条运行的构建产物，再点一下收回去。
    pub(super) fn toggle_run_build(&mut self, run_id: u64, cx: &mut Context<Self>) {
        if self.expanded_run == Some(run_id) {
            self.expanded_run = None;
            cx.notify();
            return;
        }
        self.expand_run_build(run_id, cx);
    }

    /// 展开的条件是**真有可下载的构建产物**：先问 GitHub，有产物才展开；没有就只在
    /// 状态栏说一声，不往表格里塞一行空详情。
    fn expand_run_build(&mut self, run_id: u64, cx: &mut Context<Self>) {
        if self.expanded_run == Some(run_id) {
            return;
        }
        let known = self
            .run_artifacts
            .get(&run_id)
            .map(|artifacts| (artifacts.state, artifacts.artifacts.is_empty()));
        match known {
            // 已经在路上了：这颗按钮再点不算数。
            Some((LoadState::Loading, _)) => {}
            Some((LoadState::Loaded, empty)) if !empty => {
                self.expanded_run = Some(run_id);
                cx.notify();
            }
            Some((LoadState::Loaded, _)) => {
                self.push_notice(NoticeKind::Warning, labels::RUNS_BUILD_EMPTY, cx);
            }
            _ => {
                self.run_artifacts.insert(
                    run_id,
                    RunArtifacts {
                        state: LoadState::Loading,
                        artifacts: Vec::new(),
                    },
                );
                self.load_run_artifacts(run_id, cx);
            }
        }
    }

    /// 去 GitHub 问一条运行的构建产物。
    fn load_run_artifacts(&mut self, run_id: u64, cx: &mut Context<Self>) {
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let workspace = self.workspace.clone();
        let status = self.status.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let repository = { workspace.lock().await.repository().map(str::to_owned) };
            let token = { manager.lock().await.token() };
            let (Some(repository), Some(token)) = (repository, token) else {
                return;
            };
            let Some((owner, name)) = crate::github::split_full_name(&repository) else {
                return;
            };

            let task = runtime.spawn({
                let gateway = gateway.clone();
                let token = token.clone();
                let owner = owner.clone();
                let name = name.clone();
                async move { gateway.list_artifacts(&token, &owner, &name, run_id).await }
            });

            let fetched = task.await.ok().and_then(Result::ok);
            let artifacts = match fetched {
                Some(artifacts) => RunArtifacts {
                    state: LoadState::Loaded,
                    artifacts,
                },
                None => RunArtifacts {
                    state: LoadState::Failed(AppProblem::Unexpected),
                    artifacts: Vec::new(),
                },
            };
            let opens = run_build_opens(&artifacts);
            if !opens {
                let (kind, text) = match artifacts.state {
                    LoadState::Loaded => (NoticeKind::Warning, labels::RUNS_BUILD_EMPTY),
                    _ => (NoticeKind::Error, labels::RUNS_BUILD_FAILED),
                };
                status.lock().await.push(Notice {
                    kind,
                    text: text.to_owned(),
                });
            }
            if let Err(error) = this.update(cx, |this, cx| {
                this.run_artifacts.insert(run_id, artifacts);
                // 有产物才展开：这就是"点已完成"之后展不展开的那个条件。
                if opens {
                    this.expanded_run = Some(run_id);
                }
                cx.notify();
            }) {
                warn!(?error, "the view was gone before the update landed");
            }

            Self::refresh_status(&status, &this, cx).await;
        })
        .detach();
    }

    /// 状态栏里留一句话，然后把状态栏重新读一遍。
    fn push_notice(&mut self, kind: NoticeKind, text: &'static str, cx: &mut Context<Self>) {
        let notice = Notice {
            kind,
            text: text.to_owned(),
        };
        let status = self.status.clone();
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

/// 这一份取回来的东西值不值得展开：拿到了、并且真有可下载的构建产物才展开。
///
/// 这是「点已完成」之后展不展开的唯一规则，所以单独拎出来。
pub(super) fn run_build_opens(artifacts: &RunArtifacts) -> bool {
    artifacts.state == LoadState::Loaded && !artifacts.artifacts.is_empty()
}

/// 下载这件事现在走到哪一步：和哪条产物无关的状态，摆在详情最后一行。
fn download_status(download: &DownloadState) -> Option<String> {
    match download {
        DownloadState::Downloading => Some(labels::DOWNLOADING.to_owned()),
        DownloadState::Saved(path) => {
            Some(format!("{}：{}", labels::DOWNLOAD_SAVED, path.display()))
        }
        DownloadState::Failed(problem) => Some(problem_text(*problem).to_owned()),
        DownloadState::Idle | DownloadState::NeedsConfirmation(_) => None,
    }
}
