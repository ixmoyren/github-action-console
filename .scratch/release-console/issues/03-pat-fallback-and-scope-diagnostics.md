# 03: PAT 兜底与授权诊断

**What to build:** 当未配置 OAuth App 的 client_id、或 Device Flow 因任何原因不可用时，登录页自动降级为粘贴 Personal Access Token 的路径，并说明降级原因；PAT 登录成功后同样持久化并进入应用。当授权范围不足时，明确告诉用户缺少哪个 scope（粗粒度 `repo` / `workflow`，或细粒度 `Contents: RW` / `Actions: RW` / `Workflows: RW`），而不是返回一个裸 403。

**Blocked by:** 02（Device Flow 登录与凭据持久化）

**Status:** ready-for-agent

- [ ] 未配置 client_id 或 Device Flow 不可用时，登录页提供 PAT 输入路径并显示降级原因
- [ ] PAT 登录成功后凭据同样写入钥匙串，重启免登录
- [ ] 权限不足时提示缺失的 scope（粗粒度与细粒度两种说法都覆盖）
- [ ] 降级路径与 scope 诊断由 fake gateway 覆盖测试，含"权限不足"与"降级原因"两类响应
