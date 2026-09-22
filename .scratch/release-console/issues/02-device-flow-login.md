# 02: Device Flow 登录与凭据持久化

**What to build:** 用户在登录页发起 GitHub Device Flow，看到 GitHub 给出的一次性 user code 与验证地址，可复制并在浏览器打开，同时看到等待状态与剩余有效期；授权成功后进入应用并显示当前登录用户。token 只写进操作系统钥匙串，账号元数据写进 SQLite；重启应用免登录，登出清除凭据，持久化 token 失效时被带回登录页并看到原因。这条票首次落地 `GitHubGateway` 这个唯一注入缝、octocrab 适配器、进程级 tokio runtime 与统一错误类型。

**Blocked by:** 01（应用骨架与自身构建信息）

**Status:** ready-for-agent

- [ ] 登录页展示 user code、验证地址与剩余有效期，可复制 user code 并在浏览器打开验证地址
- [ ] 授权成功后进入应用并显示当前登录用户
- [ ] token 只存 OS 钥匙串（macOS Keychain / Windows Credential Manager / Linux Secret Service），SQLite 只存账号元数据
- [ ] 重启应用免登录；登出清除钥匙串凭据
- [ ] 持久化 token 失效时回到登录页并显示原因
- [ ] `GitHubGateway` port trait 落地，app 层只依赖该 trait，octocrab 适配器保持薄映射
- [ ] 登录状态机（等待中 / 成功 / 过期 / 被拒绝 / 失效恢复）由脚本化 fake gateway 驱动测试，测试不触网
