# 06: 触发表单：选版本、目标与打包配置

**What to build:** 从看板或发布目标进入的触发面板：选择源码版本（tag / 分支）、发布目标（一个或多个）、打包配置，提交即向对应 workflow 发起 `workflow_dispatch`，并记录本次 run 的 inputs，作为后续追溯的一部分。

**Blocked by:** 01（清单）、04（CI 模板）

**Status:** ready-for-agent

- [ ] 版本字段：默认取仓库最近的 tag，可手填分支 / tag
- [ ] 目标字段：清单里的发布目标列表，支持多选或 all
- [ ] 配置字段：默认用该目标在清单里声明的打包配置，可覆盖为同仓库的其他配置
- [ ] 提交后立刻进入该 run 的追踪视图
- [ ] 失败（无 workflow、无权限、目标非法）给出可读原因，不静默
- [ ] fake gateway 测试：dispatch 收到的 inputs 与表单一致；非法组合在触达 GitHub 前就被挡住

## Comments

已实现：`ReleaseBoard::trigger` 按清单发 `workflow_dispatch`（inputs：target/version/config + 配置模板里的键），本次触发记进 `build_dispatch` 表，并在 run 列表里认领 run id（`bind_dispatches`）；看板顶部有 版本 / 目标 / 触发 三件套。
