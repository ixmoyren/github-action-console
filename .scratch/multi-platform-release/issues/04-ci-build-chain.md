# 04: CI 模板：按发布目标的真实构建链路

**What to build:** `.github/workflows/release-target.yml`：`workflow_dispatch`（inputs：target / version / config）与 `push: tags: ['v*']` 双触发；`plan` 读清单产出目标矩阵；`build` 按目标真实构建并打包（web-arm dmg、web-intel dmg、windows msi、mas 未签名 pkg、linux-tar tar.gz），上传构建产物，模拟步骤用 `::notice::`/`::warning::` 标注；`release` 在 tag 触发时把产物挂到该版本的 GitHub Release。

**Blocked by:** 03（构建对象）

**Status:** ready-for-agent

- [ ] dispatch 输入：target（单个 / all）、version（缺省用 ref）、config（打包配置名）
- [ ] `plan` 在 ubuntu 上读清单，输出矩阵与版本号（不在 macOS/Windows runner 上解析 YAML）
- [ ] 每个目标独立 job，`fail-fast: false`
- [ ] 真实产物：dmg（hdiutil）、msi（WiX）、未签名 pkg（pkgbuild/productbuild）、tar.gz
- [ ] 模拟步骤显式标注（签名、公证、App Store 提交、缺证书的 Windows 签名）
- [ ] `upload-artifact` 上传构建产物；tag 触发时 `gh release create/upload` 挂发布资产
- [ ] run 的日志里能看到本次的 target / version / config（可追溯的输入）
- [ ] 至少一次真实运行记录（run 链接）留存为演示材料

## Comments

草案已写好并本地校验过 YAML：`plan`（读清单出矩阵与版本）+ `build`（按目标独立 job，fail-fast: false，dmg / 未签名 pkg / WiX msi / tar.gz，upload-artifact，模拟步骤用 ::warning:: 标注）+ `release`（tag 触发时 gh release create/upload）。构建对象是应用自身（见 03），`build` 里直接 `cargo build --release --target …`，打包前用 `--build-info` 做一次无头自检。 **仍缺一次真实运行**：需要推到有 Actions 的仓库跑一遍并留存 run 链接；Windows / macOS 两条腿依赖 gpui-pre 在对应平台上的可构建性，尚未在 runner 上验证过。

**2026-09-27 修 msi 打包**：在真仓库上手动触发时 Windows 腿的 msi 挂了，两个原因—— (1) `plan` 在"手动触发 + 非 tag"时把 `REF_NAME`（分支名 `main`）当版本号，`Product/@Version` 拿到 `main`，WiX 直接拒绝； (2) candle 失败后 light 找不到 `.wixobj`，而 PowerShell 没检查退出码，还照样打印了"msi built"。改法：版本号改为"输入 → tag 名（仅 tag 触发）→ `Cargo.toml` 的版本 → 0.0.0"，另外单独算出 WiX 要的 `x.x.x.x`（`msi_version`，去掉 v 前缀与预发布后缀、补足四段），Windows 步骤逐步检查 `$LASTEXITCODE` 并只在成功后打印 notice；
`choco install` 前面加了 `Get-Command candle` 判断，免得每次都被"已安装"的警告打扰。

**2026-09-27 回归：上面那次修好又被覆盖了**：同一天再手动触发，Windows 腿还是报
`CNDL0108`（`Product/@Version` 拿到 `main`）与 `LGHT0103`（找不到 `dist\main.wixobj`），
说明修好的只是仓库里生成出来的 `.github/workflows/release-target.yml`。模板的唯一源头是
`templates/github/workflows/release-target.yml`（`release_template::TEMPLATE` 用
`include_str!` 读它，控制台保存工作流时按"头一行仓库名 + 模板原文"重写生成物），所以后来
一次"用发布模板覆盖工作流"把生成物按旧模板重写，改动就没了。这次把版本号规则搬回模板，
再按同一规则重新生成 `.github/workflows/release-target.yml`，两份文件除首行仓库名外逐字一致。
教训：这条工作流 **只改模板**，生成物跟着走；直接改生成物会在下一次采用模板时被抹掉。

**2026-09-27 给每个构建起名**：矩阵条目默认在 Actions 里显示成一长串字段
（`build (web-arm, macos, arm64, macos-14, aarch64-apple-darwin, dmg, …)`），认不出是哪条腿。
`plan` 现在给每个条目算出 `name`（`目标名 · 平台/架构 · 产物形式`，如 `web-arm · macos/arm64 · dmg`），
`build` job 用 `name: ${{ matrix.name }}` 采纳；有清单与无清单两条分支都走同一套。同样只改模板。
