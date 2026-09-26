# 10: GitHub 构建脚本当模板，控制台直接采用

**What to build:** 把仓库里的 `.github/workflows/release-target.yml` 做成 **可复用的多平台发布模板**——Windows / macOS-Intel / macOS-Arm / Linux 一起构建，产物挂到 GitHub Release 上——并且控制台能把它直接写进任意仓库：新建工作流时「采用发布模板」，模板里带上目标仓库的名字。

**Blocked by:** 04（CI 模板）

**Status:** ready-for-agent

- [ ] 模板一次覆盖四个平台（windows x64 / macos arm64 / macos x64 / linux x64）
- [ ] 有 `.github/release-console.yml` 时以清单为准（目标 → 打包配置 → 产物形式）；没有清单时按四个内建平台各构建一次，任何 Cargo 仓库可直接采用
- [ ] 构建对象由 `Cargo.toml` 推出（`[[bin]]` 名，退回包名），不写死项目名
- [ ] 打包前跑一次无头自检（`--build-info`），认得不认得的二进制都不挡路
- [ ] tag 触发时把所有产物 `gh release create/upload` 挂到该版本的 Release
- [ ] 控制台侧：模板是同一份文件（`include_str!`），「采用发布模板」把带仓库名的文本填进编辑器，可直接保存到目标仓库
- [ ] 单元测试：模板渲染里有四个平台、有 Release 上传、路径正确

## Comments

已实现：`.github/workflows/release-target.yml` 重写成一份 **可复用的多平台模板**——`plan` 从 `Cargo.toml` 推构建对象名（`[[bin]]` 优先，退回包名），有 `.github/release-console.yml` 时按清单出矩阵（web-arm / web-intel / windows / mas / linux-tar），没有清单时按内建的四个平台（macos-arm / macos-intel / windows / linux）各构建一次；`build` 逐目标独立 job（`fail-fast: false`），构建完用 `--build-info` 做无头自检，再打包 dmg / 未签名 pkg / msi（没有 `packaging/windows/main.wxs` 就退回 zip）/ tar.gz 并上传构建产物；`release` 在 tag
触发（或手动勾 `publish`）时 `gh release create/upload` 把四个平台的产物挂到该版本 Release 上。plan 的三种模式（有清单 / 无清单 / tag+target 过滤）都用同一段 Python 在本地跑通过。
控制台侧：`src/release_template.rs` 用 `include_str!` 编译进同一份文件（ADR-0006），`for_repository()` 在抬头写上目标仓库名；工作流页新增「发布模板」按钮，模板占用编辑器那块地方、可直接编辑，`保存模板`走 `Workspace::push_workflow_file` 写到 `.github/workflows/release-target.yml`（已有就覆盖，覆盖时带原 revision）。测试：`src/release_template.rs` 4 个单元测试（四个平台 / Release 上传 / 仓库名 / 路径）、`tests/workspace_flow.rs` 新建与覆盖两例、`src/ui/workspace.rs` 的 UI 开关测试。
