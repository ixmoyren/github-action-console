use super::AppView;
use super::*;

use super::downloads::download_controls;

impl AppView {
    pub(super) async fn refresh_detail(
        detail: &Arc<Mutex<RunDetail>>,
        this: &WeakEntity<AppView>,
        cx: &mut AsyncApp,
    ) {
        let guard = detail.lock().await;
        let open_run = guard.run_id();
        let html_url = guard.html_url().map(str::to_owned);
        let jobs = guard.jobs().to_vec();
        let jobs_state = guard.state();
        let selected_job = guard.selected_job();
        let logs = guard.logs().map(str::to_owned);
        let logs_state = guard.logs_state();
        drop(guard);

        if let Err(error) = this.update(cx, |this, cx| {
            this.open_run = open_run;
            this.run_html_url = html_url;
            this.jobs = jobs;
            this.jobs_state = jobs_state;
            this.selected_job = selected_job;
            this.logs = logs;
            this.logs_state = logs_state;
            cx.notify();
        }) {
            warn!(?error, "the view was gone before the update landed");
        };
    }
    pub(super) fn open_run_detail(&mut self, run: WorkflowRun, cx: &mut Context<Self>) {
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let detail = self.detail.clone();
        let workspace = self.workspace.clone();
        let board = self.board.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let repository = { workspace.lock().await.repository().map(str::to_owned) };
            let Some(repository) = repository else {
                return;
            };

            let task = runtime.spawn({
                let detail = detail.clone();
                let run = run.clone();
                async move {
                    detail.lock().await.open(&repository, &run);
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            Self::refresh_detail(&detail, &this, cx).await;

            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };
            let task = runtime.spawn({
                let gateway = gateway.clone();
                let detail = detail.clone();
                let token = token.clone();
                async move {
                    detail.lock().await.load_jobs(&*gateway, &token).await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            Self::refresh_detail(&detail, &this, cx).await;

            // 这次运行产出了什么：只有控制台触发过的运行才认得出它的版本与目标。
            let run_id = run.id;
            let facts = runtime
                .spawn({
                    let gateway = gateway.clone();
                    let board = board.clone();
                    let token = token.clone();
                    async move {
                        board
                            .lock()
                            .await
                            .facts_for_run(&*gateway, &token, run_id)
                            .await
                    }
                })
                .await
                .unwrap_or_else(|error| {
                    warn!(%error, "a background task did not finish");
                    None
                });

            if let Err(error) = this.update(cx, |this, cx| {
                this.release_facts = facts;
                this.artifacts_state = LoadState::Loading;
                this.load_artifacts(cx);
            }) {
                warn!(?error, "the view was gone before the update landed");
            };
        })
        .detach();
    }
    pub(super) fn close_run(&mut self, cx: &mut Context<Self>) {
        let detail = self.detail.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let detail = detail.clone();
                async move {
                    detail.lock().await.close();
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }

            if let Err(error) = this.update(cx, |this, cx| {
                this.open_run = None;
                this.run_html_url = None;
                this.jobs.clear();
                this.jobs_state = LoadState::Idle;
                this.selected_job = None;
                this.logs = None;
                this.logs_state = LoadState::Idle;
                this.logs_copied = false;
                this.artifacts.clear();
                this.artifacts_state = LoadState::Idle;
                this.release_facts = None;
                this.download_state = DownloadState::Idle;
                cx.notify();
            }) {
                warn!(?error, "the view was gone before the update landed");
            };
        })
        .detach();
    }
    pub(super) fn select_job(&mut self, job_id: u64, cx: &mut Context<Self>) {
        let gateway = self.gateway.clone();
        let manager = self.manager.clone();
        let detail = self.detail.clone();
        let runtime = self.runtime.clone();
        cx.spawn(async move |this, cx| {
            let task = runtime.spawn({
                let detail = detail.clone();
                async move {
                    detail.lock().await.select_job(job_id);
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
            Self::refresh_detail(&detail, &this, cx).await;

            let token = { manager.lock().await.token() };
            let Some(token) = token else {
                return;
            };
            let task = runtime.spawn({
                let gateway = gateway.clone();
                let detail = detail.clone();
                async move {
                    detail.lock().await.load_logs(&*gateway, &token).await;
                }
            });
            if let Err(error) = task.await {
                warn!(%error, "a background task did not finish");
            }
            Self::refresh_detail(&detail, &this, cx).await;
        })
        .detach();
    }

    /// 每个 job 的步骤各装进一个编辑器。
    ///
    /// job 从网络来，而编辑器只能在有 window 的帧里造，所以和 workflow 文件那次一样
    /// 在 render 里对账：新来的 job 建一个，文本变了才写进去，走掉的 job 收掉。
    pub(super) fn sync_job_editors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let jobs = self
            .jobs
            .iter()
            .map(|job| (job.id, job_steps_text(job)))
            .collect::<Vec<_>>();

        for (id, text) in jobs {
            let editor = match self.job_step_editors.get(&id).cloned() {
                Some(editor) => editor,
                None => {
                    let editor = cx.new(|cx| yaml_editor::plain_editor_state(window, cx));
                    self.job_step_editors.insert(id, editor.clone());
                    editor
                }
            };
            let applied = self.job_step_texts.entry(id).or_default();
            super::workspace::push_editor_text(&editor, applied, text, window, cx);
        }

        // 换一次运行、或者某个 job 不在了，就把它那一份编辑器丢掉。
        let live = self.jobs.iter().map(|job| job.id).collect::<HashSet<_>>();
        self.job_step_editors.retain(|id, _| live.contains(id));
        self.job_step_texts.retain(|id, _| live.contains(id));
    }

    pub(super) fn run_detail_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = div().flex().flex_col().gap_3();

        let open_url = self.run_html_url.clone();
        panel = panel.child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .child(
                    Button::new("close-run")
                        .label(labels::RUN_DETAIL_BACK)
                        .on_click(cx.listener(|this, _, _, cx| this.close_run(cx))),
                )
                .child(
                    Button::new("open-run-browser")
                        .label(labels::RUN_DETAIL_OPEN_BROWSER)
                        .on_click(move |_, _, _| {
                            if let Some(url) = open_url.clone() {
                                let _ = open::that(url);
                            }
                        }),
                ),
        );

        panel = panel.child(pickable(labels::JOBS_TITLE));
        match self.jobs_state {
            LoadState::Loading if self.jobs.is_empty() => {
                panel = panel.child(pickable(labels::JOBS_LOADING));
            }
            LoadState::Failed(problem) => {
                panel = panel.child(Label::new(problem_text(problem)).text_sm());
            }
            LoadState::Loaded if self.jobs.is_empty() => {
                panel = panel.child(pickable(labels::JOBS_EMPTY));
            }
            _ => {}
        }

        let job_rows = self
            .jobs
            .iter()
            .map(|job| {
                let id = job.id;
                let summary = format!(
                    "{}｜{}｜{}",
                    job.name,
                    job.status.label(),
                    job.conclusion
                        .clone()
                        .unwrap_or_else(|| labels::VALUE_MISSING.to_owned()),
                );
                // 步骤交给编辑器：行号、选择、复制都跟着它走。编辑器还没建出来时
                // （换运行的那一帧）退回一行纯文本，别让这一段空着。
                let steps_text = job_steps_text(job);
                let steps = match self.job_step_editors.get(&id) {
                    Some(editor) => Editor::new(editor)
                        .h(steps_editor_height(editor, job.steps.len(), cx))
                        .readonly(true)
                        .into_any_element(),
                    None => plain_steps(&steps_text).into_any_element(),
                };

                div()
                    .id(SharedString::from(format!("job-card-{id}")))
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .child(pickable(summary))
                            .child(
                                Button::new(SharedString::from(format!("job-{id}")))
                                    .label(labels::JOB_VIEW_LOGS)
                                    .on_click(
                                        cx.listener(move |this, _, _, cx| this.select_job(id, cx)),
                                    ),
                            ),
                    )
                    .child(Label::new(labels::JOB_STEPS).text_sm())
                    .child(steps)
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        panel = panel.child(div().flex().flex_col().gap_2().children(job_rows));

        let log_text = self.logs.clone();
        let query = self.log_input.read(cx).value().to_string();
        let filtered = log_text.as_deref().map(|log| filter_log_lines(log, &query));

        panel = panel.child(pickable(labels::LOGS_TITLE));
        panel = panel.child(Input::new(&self.log_input));

        match filtered {
            Some(text) => {
                let body = if text.is_empty() {
                    labels::LOGS_EMPTY.to_owned()
                } else {
                    text.clone()
                };
                let copy_label = if self.logs_copied {
                    labels::LOGS_COPIED
                } else {
                    labels::LOGS_COPY
                };
                panel = panel.child(Button::new("copy-logs").label(copy_label).on_click(
                    cx.listener(move |this, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                        this.logs_copied = true;
                        cx.notify();
                    }),
                ));
                panel = panel.child(
                    div()
                        .h(px(360.0))
                        .overflow_y_scrollbar()
                        .child(pickable(body)),
                );
            }
            None if self.logs_state == LoadState::Loading => {
                panel = panel.child(pickable(labels::LOGS_LOADING));
            }
            None => {
                if let LoadState::Failed(problem) = self.logs_state {
                    panel = panel.child(Label::new(problem_text(problem)).text_sm());
                }
            }
        }

        panel = panel.child(pickable(labels::ARTIFACTS_TITLE));
        match self.artifacts_state {
            LoadState::Loading if self.artifacts.is_empty() => {
                panel = panel.child(pickable(labels::ARTIFACTS_LOADING));
            }
            LoadState::Failed(problem) => {
                panel = panel.child(Label::new(problem_text(problem)).text_sm());
            }
            LoadState::Loaded if self.artifacts.is_empty() => {
                panel = panel.child(pickable(labels::ARTIFACTS_EMPTY));
            }
            _ => {}
        }

        let artifact_rows = self
            .artifacts
            .iter()
            .map(|artifact| {
                let artifact = artifact.clone();
                let expired = if artifact.expired {
                    format!("（{}）", labels::ARTIFACTS_EXPIRED)
                } else {
                    String::new()
                };
                let mut row = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(pickable(format!(
                        "{}｜{} B{}",
                        artifact.name, artifact.size_in_bytes, expired
                    )));
                let artifact_id = artifact.id;
                let downloading = artifact.clone();
                row = row.child(download_controls(
                    &self.download_state,
                    &DownloadKind::Artifact { artifact_id },
                    SharedString::from(format!("artifact-{artifact_id}")),
                    labels::ARTIFACT_DOWNLOAD,
                    cx.listener(move |this, _, _, cx| {
                        this.download_artifact(downloading.clone(), cx)
                    }),
                    cx,
                ));
                row.into_any_element()
            })
            .collect::<Vec<_>>();
        panel = panel.child(div().flex().flex_col().gap_2().children(artifact_rows));

        // 三个事实分开写：跑完了 / 能下载什么 / 有没有登记发布。
        let open_run = self
            .open_run
            .and_then(|id| self.runs.iter().find(|run| run.id == id));
        panel = panel.child(pickable(labels::RELEASE_FACTS_TITLE));
        match &self.release_facts {
            Some(facts) => {
                panel = panel.child(pickable(format!(
                    "{}：{}｜{}：{}｜{}：{}",
                    labels::RELEASE_FACTS_VERSION,
                    facts.version,
                    labels::RELEASE_FACTS_TARGET,
                    facts.target,
                    labels::RELEASE_FACTS_CONFIG,
                    facts.config
                )));
                panel = panel.child(
                    Label::new(format!(
                        "{}：{}",
                        labels::RELEASE_FACTS_BUILT,
                        run_state_text(open_run)
                    ))
                    .text_sm(),
                );
                panel = panel.child(
                    Label::new(format!(
                        "{}：{}",
                        labels::RELEASE_FACTS_ASSETS,
                        facts_assets_text(facts, &self.artifacts)
                    ))
                    .text_sm(),
                );
                // 发布资产逐条列出：名字、大小、时间，各给一个下载入口——控制台里
                // 就能把 Release 上的包拿到本地，不必跳去网页。
                let asset_rows = facts
                    .assets
                    .iter()
                    .map(|asset| self.release_asset_row(asset, cx))
                    .collect::<Vec<_>>();
                if !asset_rows.is_empty() {
                    panel = panel.child(div().flex().flex_col().gap_1().children(asset_rows));
                }
                panel = panel.child(
                    Label::new(format!(
                        "{}：{}",
                        labels::RELEASE_FACTS_PUBLISHED,
                        facts_published_text(facts)
                    ))
                    .text_sm(),
                );
                if let Some(problem) = facts.problem {
                    panel = panel.child(Label::new(problem_text(problem)).text_sm());
                }
            }
            None => {
                panel = panel.child(Label::new(labels::RELEASE_FACTS_UNKNOWN).text_sm());
            }
        }

        panel = panel.child(download_controls(
            &self.download_state,
            &DownloadKind::RunLogs {
                run_id: self.open_run.unwrap_or_default(),
            },
            SharedString::from("download-run-logs"),
            labels::RUN_LOGS_DOWNLOAD,
            cx.listener(|this, _, _, cx| this.download_run_logs(cx)),
            cx,
        ));

        // 整个 jobs 页都在一个可滚动容器里：job 卡片、步骤编辑器、日志、产物都跟着
        // 这一条滚。抽屉本身高度固定，滚动的是它里面这一层。
        panel
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .id("run-detail-scroll")
            .into_any_element()
    }

    /// 一条发布资产：名字、大小、时间，加一个下载入口。
    fn release_asset_row(&self, asset: &ReleaseAsset, cx: &mut Context<Self>) -> AnyElement {
        let when = asset
            .created_at
            .as_deref()
            .map(super::repositories::format_commit_date)
            .unwrap_or_else(|| labels::VALUE_MISSING.to_owned());
        let asset = asset.clone();

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .child(pickable(format!(
                "{}｜{} B（{}）",
                asset.name, asset.size_in_bytes, when
            )))
            .child(
                Button::new(SharedString::from(format!("release-asset-{}", asset.name)))
                    .label(labels::ARTIFACT_DOWNLOAD)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.download_release_asset(asset.clone(), cx)
                    })),
            )
            .into_any_element()
    }
}

/// gpui-component 的编辑器行高按字体尺寸的 1.5 倍算；主题还没量出来时先用等宽字体的
/// 默认行高兜着。
const STEPS_LINE_HEIGHT: f32 = 20.0;

/// 编辑器自己的边框与内边距：它不随步数变，量出来的高度减去行高就是这个数。
const STEPS_EDITOR_PADDING: f32 = 18.0;

/// 步骤编辑器要占多高：行高 × 步数，加上编辑器自己的边框与内边距。
///
/// gpui-component 的编辑器不会自己长高（不给高度就只露一行），而整页是可滚动的，
/// 所以这里按步数把高度算足：一个 job 的步骤要么一眼看完，要么整页滚下去看。
fn steps_editor_height(editor: &Entity<EditorState>, rows: usize, cx: &App) -> Pixels {
    let line = editor
        .read(cx)
        .line_height()
        .unwrap_or(px(STEPS_LINE_HEIGHT));
    line * rows.max(1) as f32 + px(STEPS_EDITOR_PADDING)
}

/// 一个 job 的步骤，一行一条：编号、名字、状态（有结论就报结论，否则报它还在做什么）。
fn job_steps_text(job: &Job) -> String {
    job.steps
        .iter()
        .map(|step| format!("{} {} · {}", step.number, step.name, step_state_text(step)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 一步的状态：跑完了说结论，"进行中"这类还没结论的步就说它自己的状态。
fn step_state_text(step: &Step) -> String {
    match step.conclusion.as_deref().filter(|value| !value.is_empty()) {
        Some(conclusion) => labels::conclusion_label(Some(conclusion)),
        None => step.status.label().to_owned(),
    }
}

/// 步骤编辑器的兜底：编辑器还没建出来时，同一份步骤先当一行纯文本摆着。
fn plain_steps(text: &str) -> Label {
    let text = if text.is_empty() {
        labels::VALUE_MISSING.to_owned()
    } else {
        text.replace('\n', " / ")
    };
    Label::new(text).text_sm()
}

/// 构建完成这一条：这次运行自己的状态与结论。
fn run_state_text(run: Option<&WorkflowRun>) -> String {
    match run {
        Some(run) => format!(
            "{}（{}）",
            labels::conclusion_label(run.conclusion.as_deref()),
            run.status.label()
        ),
        None => labels::VALUE_MISSING.to_owned(),
    }
}

/// 产物可获取这一条：构建产物（临时）与发布资产（可下载）分别多少。
fn facts_assets_text(facts: &ReleaseFacts, artifacts: &[BuildArtifact]) -> String {
    let expired = artifacts.iter().filter(|artifact| artifact.expired).count();
    let mut text = format!(
        "{} {}｜{} {}",
        labels::ARTIFACTS_TITLE,
        artifacts.len(),
        labels::RELEASE_ASSETS_LABEL,
        facts.assets.len()
    );
    if expired > 0 {
        text.push_str(&format!("（{} 个 {}）", expired, labels::ARTIFACTS_EXPIRED));
    }
    text
}

/// 已登记发布这一条：哪些通道现在指向这个版本。
fn facts_published_text(facts: &ReleaseFacts) -> String {
    if facts.pointers.is_empty() {
        return labels::RELEASE_FACTS_NOTHING.to_owned();
    }
    facts
        .pointers
        .iter()
        .map(|pointer| {
            format!(
                "{} {}（{}）",
                labels::RELEASE_FACTS_CHANNEL,
                pointer.channel,
                super::repositories::format_commit_date(&pointer.updated_at)
            )
        })
        .collect::<Vec<_>>()
        .join("｜")
}
