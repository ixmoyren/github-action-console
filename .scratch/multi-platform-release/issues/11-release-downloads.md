# 11: 构建记录与 Release 包下载

**What to build:** 每次构建都留下记录（谁触发、哪个版本、哪个目标、哪个配置、什么时候、结果如何），并且控制台能直接把该版本 Release 上的包 **下载到本地**——不必跳去 GitHub 网页。构建产物会过期，Release 里的发布资产不会，所以长期留存看的是后者。

**Blocked by:** 04（CI 模板）、07（构建追踪）

**Status:** ready-for-agent

- [ ] 运行记录里能看到这次构建的输入（版本 / 目标 / 配置）与三个事实（已完成 / 产物可获取 / 已登记发布）
- [ ] 发布资产逐条列出名字、大小、时间，并各给一个下载入口
- [ ] 大文件先确认再下载（沿用既有的确认流程），下载完给出保存路径
- [ ] 下载走 gateway 的 `download_release_asset`，fake gateway 可脚本化
- [ ] 单元测试：按 URL 下载并落盘；大文件排队等确认

## Comments

已实现：运行详情的「发布事实」一节里，`产物可获取` 现在只报数量（构建产物几个、其中过期几个、发布资产几个），发布资产逐条列出 **名字 / 大小 / 时间**并各带一个「下载」按钮。下载走 gateway 新增的 `download_release_asset(token, url)`（`_get_with_headers` 收全响应体，跟随 GitHub 到对象存储的跳转），落到 `Downloads` 里新加的 `DownloadKind::ReleaseAsset`：小文件直接下、大文件沿用既有的确认流程（`needs_confirmation_for` + `queue_release_asset`），文件名就是 Release 上的名字。测试：`tests/downloads_flow.rs` 三例（按名字落盘并断言用了哪个
URL / 大文件先确认 / 没有下载地址就报失败）。
构建记录本身沿用运行列表与 `build_dispatch` 表：谁触发、哪个版本 / 目标 / 配置，以及三个事实，运行时都留在记录里。
