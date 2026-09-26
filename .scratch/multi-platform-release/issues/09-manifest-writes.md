# 09: 清单写入走分支 + PR

**What to build:** 在控制台里改动发布清单（新增/修改发布目标或打包配置）时，改动落在新分支并开成 PR，绝不直接写默认分支（ADR-0004）；仓库还没有清单时，同样走这条路生成初版。

**Blocked by:** 01（清单）

**Status:** ready-for-agent

- [ ] 编辑发布目标 / 打包配置的入口，改动以 diff 形式可读
- [ ] 提交落在新分支，并创建 PR（标题与说明写明是发布清单变更）
- [ ] 默认分支受保护时给出清晰错误而不是静默失败
- [ ] 仓库无清单时生成初版清单（含示例目标）并走同一路径
- [ ] fake gateway 测试：写入的 path / branch / PR 参数符合预期；默认分支不被直接写

## Comments

已实现：看板抽屉里多了"编辑清单"，同一个抽屉换面成编辑器（清单原文来自 `BoardSnapshot::manifest_text`），提交走 `ReleaseBoard::save_manifest`——`create_branch` 从默认分支开 `release-console/manifest-*`，`write_file` 带上原 revision（没有就是新建初版），再 `open_pull_request` 回默认分支；默认分支只当基线，从不直接写。通知里回报分支名与 PR 号。gateway 侧新增 `create_branch` / `open_pull_request`（PR 参数收成 `PullRequest`）。测试：`tests/release_flow.rs` 断言写入的 path/branch/PR 参数、默认分支未被直接写、无清单时生成初版；
`src/ui/workspace.rs` 有编辑器开关的 UI 测试。 **未做**：把清单改动做成 diff 预览（改完直接提交，没有逐行对比）。
