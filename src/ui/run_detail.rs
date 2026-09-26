use super::AppView;
use super::*;

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
                let steps = job
                    .steps
                    .iter()
                    .map(|step| {
                        format!(
                            "{} {} {}",
                            step.number,
                            step.name,
                            step.conclusion
                                .clone()
                                .unwrap_or_else(|| labels::VALUE_MISSING.to_owned())
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" / ");

                div()
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
                    .child(Label::new(format!("{}：{}", labels::JOB_STEPS, steps)).text_sm())
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
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(pickable(format!(
                        "{}｜{} B{}",
                        artifact.name, artifact.size_in_bytes, expired
                    )))
                    .child(
                        Button::new(SharedString::from(format!("artifact-{}", artifact.id)))
                            .label(labels::ARTIFACT_DOWNLOAD)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.download_artifact(artifact.clone(), cx)
                            })),
                    )
                    .into_any_element()
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

        panel = panel.child(
            Button::new("download-run-logs")
                .label(labels::RUN_LOGS_DOWNLOAD)
                .on_click(cx.listener(|this, _, _, cx| this.download_run_logs(cx))),
        );

        match &self.download_state {
            DownloadState::NeedsConfirmation(pending) => {
                let size = pending.size_in_bytes.unwrap_or_default();
                panel = panel.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(pickable(format!(
                            "{}（{} B）",
                            labels::DOWNLOAD_CONFIRM_TITLE,
                            size
                        )))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap_2()
                                .child(
                                    Button::new("confirm-download")
                                        .label(labels::DOWNLOAD_CONFIRM)
                                        .primary()
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.confirm_download(cx)),
                                        ),
                                )
                                .child(
                                    Button::new("cancel-download")
                                        .label(labels::DOWNLOAD_CANCEL)
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.cancel_download(cx)),
                                        ),
                                ),
                        ),
                );
            }
            DownloadState::Downloading => {
                panel = panel.child(pickable(labels::DOWNLOADING));
            }
            DownloadState::Saved(path) => {
                panel = panel.child(pickable(format!(
                    "{}：{}",
                    labels::DOWNLOAD_SAVED,
                    path.display()
                )));
            }
            DownloadState::Failed(problem) => {
                panel = panel.child(Label::new(problem_text(*problem)).text_sm());
            }
            DownloadState::Idle => {}
        }

        panel.into_any_element()
    }
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
    for asset in &facts.assets {
        let when = asset
            .created_at
            .as_deref()
            .map(super::repositories::format_commit_date)
            .unwrap_or_else(|| labels::VALUE_MISSING.to_owned());
        text.push_str(&format!("\n· {}（{}）", asset.name, when));
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
