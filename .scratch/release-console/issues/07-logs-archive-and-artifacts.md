# 07: 运行日志包与构建产物下载

**What to build:** 用户可以按需下载整次运行的日志包（zip），也能看到这次运行产出的构建产物列表并下载单个产物。因为 octocrab 的日志与产物下载是一次性读进内存的、没有流式接口，这条票必须走完整路径：先取大小与类型 → 超过阈值先让用户确认 → 在后台任务里写盘 → 下载期间 UI 保持可交互。本应用不上传任何二进制（ADR-0001）。

**Blocked by:** 06（运行详情与 job 日志）

**Status:** ready-for-agent

- [ ] 按需下载整次运行的日志包（zip）
- [ ] 列出该次运行产出的构建产物并支持单个下载
- [ ] ~~下载前获取大小，超过阈值时要求用户确认~~ —— 2026-09-27 改成"点下载直接下，那一行摆进度条"（见下）
- [ ] 下载在后台任务中写盘，期间 UI 保持可交互
- [ ] 下载路径（含下载失败）由 fake gateway 测试，不产生真实的大文件上传

## Comments

**2026-09-27 下载落地要弹一句，带上路径**：下载按钮点下去就是在后台开一个任务（`runtime.spawn`
里 `Downloads::perform` 写盘，UI 不挡），这一步原来就有；缺的是 **落地后的提示**——写完了只在
运行详情/展开行里默默留一行字，人容易看不见。现在每次 `refresh_downloads` 拿到落地状态时，
把一句话排进提示队列：

- 成功：`已保存到：<绝对路径>`，标题「下载完成」；
- 失败：失败原因，标题「下载失败」；
- 还在下载的时候：不弹。

弹的是 gpui-component 的提示（toast）：`AppView` 里有一个 `NotificationList`，`shell` 在两
个分支都画 `Root::render_notification_layer`。异步任务里没有 window，而 `push` 要有 window，
所以完成的提示先排在 `pending_notifications`，下一帧 render 用 `window.defer` 交给通知层
（不在画的时候新建实体）。这条对所有下载都生效（构建产物、运行日志包、发布资产），不只是产物。

覆盖测试：`a_finished_download_reports_where_the_file_landed`（纯函数 `download_announcement`
给出的文案里有保存路径；下载中/待确认不弹）与 `a_finished_download_pops_a_toast`（排队的提示
真的进了通知层）。

**2026-09-27 大文件确认换成进度条**：原来超过 50 MB 的产物/资产要先弹一句「该文件较大，确认下载？」，
再点「确认下载」才真的去取。这一步去掉：点下载就直接在后台开任务， **正在下的那一条把下载按钮换成
进度条**（gpui-component 的 `Progress::loading(true)`，不确定的那种动画——octocrab 一口气把文件读进
内存，没有字节流可以数，所以不谎报百分比）。下完照旧弹带保存路径的提示。

连带删掉的：`DownloadState::NeedsConfirmation`、`Downloads` 的大小阈值与
`queue_artifact/queue_release_asset/confirm/cancel/pending/needs_confirmation*`、确认与取消两个按钮、
三条文案（`DOWNLOAD_CONFIRM_TITLE`/`DOWNLOAD_CONFIRM`/`DOWNLOAD_CANCEL`），以及 UI 的
`confirm_download`/`cancel_download`。`DownloadState::Downloading` 现在带着 `DownloadTask`
（文件名、大小、是哪种下载），进度条靠 `downloading_artifact` 认领自己那一行；运行详情页与运行列表
的展开行都这么画。

代价说清楚： **不再有内存保护**。那道确认挡的本来就是"整个文件进内存"的风险，现在多大的产物点下去
都会整个读进来。要挡的话得加硬上限（超过就拒绝并提示），那是新决定，没做。

测试相应改过：`tests/downloads_flow.rs` 里大产物、大发布资产都改成断言"直接就下"
（`a_large_artifact_downloads_straight_away_too`、`a_large_release_asset_downloads_straight_away`），
删掉取消确认那条；UI 侧 `the_completed_run_expands_its_artifacts_in_the_table` 多断言一条：
正在下的那一行没有下载按钮、有的是进度条。

**2026-09-27 补：进度条其实一直没露过面**。上面那版改完，界面上还是"点了下载什么都没发生"——
因为视图只在下载 **结束**之后才 `refresh_downloads`，中途那个 `Downloading` 状态它从来没看见过，
于是进度条、取消、状态栏那一句全都没机会画。改法：开下之前先在 `Downloads` 上 `begin_*`（状态
落到"正在下"）， **当场** `refresh_downloads` 一次，再在后台去取。

现在的样子：

- 点击下载 → 状态栏立刻说 `文件正在写入 <绝对路径> 中`；这一行的下载按钮换成 **进度条 + 取消**。
- 取消 → 把后台任务的把手 `abort()` 掉（文件是取完才写的，所以掐在中间不会留半个文件），
  状态收回 Idle，进度条和取消消失。
- 下完 → 进度条和取消消失，那一行变成 `文件已经保存到 <绝对路径> 中` **后面跟一颗下载按钮**
  （想再下一遍就再点）；同时照旧弹带路径的提示。
- 失败 → 那一行先说明原因，后面同样跟一颗下载按钮。

为了让"哪一行认领哪个状态"成立，模型也改了：`DownloadTask` 带的是 **绝对路径**（不再只是文件名，
状态栏和行里要报路径），`DownloadState::Saved(task)` / `Failed { task, problem }` 都带着是哪一样
东西；`download_controls` 按 `kind` 认领。运行详情页的产物、运行日志包和发布资产共用这一套。

测试：`the_completed_run_expands_its_artifacts_in_the_table` 现在把这一整条链路断言下来——
下载中那一行有进度条和取消、状态栏有 `download-writing`；下完之后进度条、取消、状态栏那句都消失，
下载按钮回来。

**2026-09-27 默认下载目录改成系统「下载」目录**：原来落在应用自己的数据目录
（`<data_dir>/github-action-console/downloads`）。现在用 `dirs::download_dir()`——macOS 是
`~/Downloads`，Windows 是已知文件夹里的 Downloads，Linux 走 XDG user dirs 里的 `XDG_DOWNLOAD_DIR`。
Linux 上没配 user-dirs 时它是 `None`，那种情况退回应用自己的目录，别把文件甩到工作目录。
`dirs` 本来就是依赖（`default_store_path` 在用），所以没有新增 crate；启动时 `debug!` 记一条
下载落在哪儿，方便对照。
