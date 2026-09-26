# 03: 构建对象与它的构建配置

**What to build:** 被这条发布链路打包的那个客户端。 **本应用自身就是构建对象**（`github-action-console`）：它是 macOS / Windows / Linux 的桌面客户端，正是这套发布目标要分发的那个东西。它要报得出自己的三项事实——版本号、构建目标（target triple）、打包配置，并且这三项在无头环境里也拿得到。为此不再维护一个只为了当靶子的演示程序。

**Status:** ready-for-agent

- [ ] 构建对象是本仓库的 workspace 包，CI 直接 `cargo build --release --target <triple>`
- [ ] `build.rs` 注入版本号、target triple、打包配置标识（缺失时显示未指定）
- [ ] 界面上显示三者（`AppInfo`）
- [ ] `--build-info` 无头打印同样三项，供 CI 的打包自检使用
- [ ] arm64 / x86_64 交叉构建都能成功
- [ ] 打包模板：`packaging/windows/main.wxs`（WiX，变量化的 exe 路径与版本）

## Comments

**改过一版**：原先的 `demo-client/` 是为当靶子专门造的演示程序，用户判定无用并删除（commit `bf98690`）。构建对象改成应用自身：`build.rs` 注入的 `TARGET` 与 `GAC_PACKAGING_CONFIG` 早已存在，界面上也早就由 `AppInfo` 显示；这一版补上 `AppInfo::report()` 与 `github-action-console --build-info`，让没有窗口的 CI 也能确认"这个产物是哪个目标、哪份配置"，并有单元测试。WiX 草案移到 `packaging/windows/main.wxs`，产品名改成应用本身。CI 的构建/打包命令在 `.github/workflows/release-target.yml`。
