# 03: 示例客户端与它的构建配置

**What to build:** `demo-client/`：一个只显示版本号、构建目标（target triple）与打包配置的最小客户端（独立 Cargo 包，不加入工作区），三者由 `build.rs` 在构建期注入。它存在的意义是被打包成 dmg / msi / pkg / tar.gz，作为真实链路的构建对象。

**Status:** ready-for-agent

- [ ] `demo-client/` 可独立 `cargo build`（不依赖主程序）
- [ ] `build.rs` 注入版本号、target triple、打包配置标识（缺失时显示未指定）
- [ ] 运行输出三者；`--version` 同样输出
- [ ] `cargo install --target` 交叉构建 arm64 / x86_64 都能成功
- [ ] 打包模板：`demo-client/packaging/windows/main.wxs`（WiX，变量化的 exe 路径与版本）
- [ ] README 说明它是示例客户端、如何构建、哪里是真实/模拟
