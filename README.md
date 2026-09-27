# GitHub Action Console

一个跨平台的桌面控制台：登录 GitHub 之后选仓库，看工作流与运行、触发构建、读日志、拿产物，
并用 **通道 tag** 把版本推进到 LTS / latest / dogfood 这些通道上。

> 状态：早期（0.1.0），界面文案是中文。
> 英文版见 [README.en.md](README.en.md)；逐步走查见[用户手册](docs/user-manual.md)。

## 它把什么收进一个窗口

发布一个桌面软件，原来要在三个互不相连的界面之间来回：Actions 里翻 run 和日志、Releases 里核对
产物与下载链接、以及心里（或一张表里）记着各通道现在指向哪个版本。这个控制台把这条链路连起来：

- **登录与凭据**：GitHub Device Flow 登录（OAuth App），Personal Access Token 兜底。
  token 只写进操作系统钥匙串（macOS Keychain / Windows Credential Manager / Linux Secret Service），
  重启免登录，登出即清除。
- **仓库**：列出你名下的仓库（含私有），按名称搜索、按最近更新或最近推送排序、分页加载。
- **工作流**：列出仓库里的 workflow；打开某一个读它的 YAML（带高亮、可改），保存即提交到默认分支；
  另有「填表生成 workflow」和「采用发布模板」两个入口。
- **运行**：运行记录列表（运行状态 / 结果 / 分支 / 触发方式 / 时间，可排序、可按分支过滤，
  未跑完的能取消、跑完了的能删除）；运行详情里有每个 job 的步骤、原始日志（可搜索、可复制）、
  构建产物（可下载）与这次运行的发布事实。
- **发布看板**：行是发布目标（清单里定义的），列是通道 tag（LTS / latest / dogfood）。
  选分支 → 选提交 → 选版本（通道）→ 点「创建」，就在仓库里建好（或移动）同名 tag。tag 推上去会
  触发发布工作流，看板按这个 tag 认领那次运行，逐行显示 windows / macOS / Linux 各自 job 的状态。
- **下载**：构建产物、运行日志包、发布资产都下到系统的「下载」目录；下载中有进度条与取消，
  完成后弹出提示并写出 **保存到哪儿的绝对路径**。
- **始终看得见的事实**：状态栏显示当前登录用户与 GitHub 限流余量；`--build-info` 报出这个
  二进制自己的版本号、构建目标与打包配置（CI 的无头自检读的也是它）。

## 它不做什么

- **不构建、不上传任何二进制**（[ADR-0001](docs/adr/0001-ci-builds-and-uploads-release-assets.md)）。
  构建、打包、上传都由 GitHub Actions 承担，控制台只负责触发与观察。
- 签名、公证、App Store 提交没有证书与账号，一律标注为 **模拟**，不会产出"已完成"的事实。
- 不做 GitHub 全能客户端：只管这个仓库的工作流、运行、产物与通道。
- 不替你决定发布策略：哪个通道指向哪个提交，由你在看板上点。
- 本仓库暂未附许可证文件。

## 快速开始

```bash
cargo run                     # 开窗口
cargo run -- --build-info     # 不开窗口，只报"我是哪个构建"
cargo test --all --workspace  # 测试
```

依赖：Rust（edition 2024）。Linux 上还要装上窗口系统与字体相关的开发库，CI 里用的是：

```bash
sudo apt-get install -y --no-install-recommends \
  pkg-config clang libclang-dev \
  libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev \
  libx11-dev libxcb1-dev libxcb-randr0-dev libxcb-shape0-dev \
  libxcb-xfixes0-dev libxcb-xkb-dev libfontconfig1-dev libssl-dev
```

登录需要 GitHub 凭据，二选一：

- **Device Flow**：用自己的 OAuth App 的 client id（在登录页填一次）。
- **PAT**：粗粒度 token 需要 `repo` + `workflow`；细粒度 token 需要 Contents、Actions、
  Workflows 三项读写。

要连内网或走代理：在 **登录页**右上角的齿轮（设置）里填代理地址——它挂在登录页上，所以登录后
想改要先登出。

## 发布清单：仓库里的一份 YAML

哪个仓库有哪些发布目标、每个目标怎么打包，由仓库根的 `.github/release-console.yml` 决定
（[ADR-0003](docs/adr/0003-repo-release-manifest-is-source-of-truth.md)）。本仓库自己那份长这样：

```yaml
version: 1

targets:
  windows:
    platform: windows          # macos / windows / linux
    arch: x64                  # arm64 / x64 / universal
    distribution: github-releases
    packaging: windows-msi     # 指向 packaging 里的一个配置
    simulated: # 可选：这几步在这里只是模拟
      - code-signing

packaging:
  windows-msi:
    workflow: release-target.yml
    inputs:
      package: msi             # dmg / msi / pkg / tar.gz
```

没有这份清单的仓库也能用：控制台退回到内建的四个平台（macos-arm / macos-intel / windows /
linux）各构建一次。目标名只能用字母、数字、点和减号。

## 发布工作流模板

[templates/github/workflows/release-target.yml](templates/github/workflows/release-target.yml)
是本仓库的发布脚本，也是控制台「采用发布模板」时写进别的仓库的那一份（`include_str!` 同一份文本，
[ADR-0006](docs/adr/0006-the-build-script-is-the-template.md)）。要点：

- **双触发**：`workflow_dispatch`（选目标 / 版本 / 打包配置手动跑）与 `push: tags: ['**']`
  （任何 tag 都跑：通道 tag 与版本 tag 一视同仁）。
- **一份清单出一张矩阵**：`plan` 读清单与 `Cargo.toml`，每个发布目标一条，`fail-fast: false`。
- **版本号规则**：dispatch 输入优先；tag 名像版本（`v1.2.3` / `1.2.3`）就用它，否则用
  `Cargo.toml` 里的版本（所以 `latest` 这种通道 tag 构建出的产物仍叫 `…-0.1.0-…`）。
  Windows Installer 另算一个合法的 `x.x.x.x`。
- **产物**：每个目标 `cargo build --release` → `--build-info` 自检 → 打包（dmg / msi / 未签名
  pkg / tar.gz / zip）→ `upload-artifact`。
- **发布资产**：tag 触发时挂到 **触发这次构建的那个 tag** 的 Release 上（通道 tag 的资产就是这条
  通道现在指的那次构建）。
- 工作流名、目标名、产物名都从清单与 Cargo.toml 推出来，所以换个 Cargo 仓库就能直接用。

## 目录结构

```
src/app/       用例层：登录、仓库、工作流、运行、下载、发布看板（不依赖 GPUI）
src/github/    GitHubGateway 端口与 octocrab 适配器（唯一的注入缝）
src/store.rs   SQLite：上次选的仓库、本地缓存
src/ui/        GPUI Kit 视图：shell / repositories / workspace / run_detail / downloads / status
src/release.rs 发布模型：清单解析、状态推导（纯函数）
src/app_info.rs 这个二进制是谁：版本号 / 构建目标 / 打包配置
templates/     发布工作流模板（也是本仓库自己用的那一份）
packaging/     平台打包文件（如 Windows 的 WiX 定义）
docs/          ADR、用户手册；CONTEXT.md 是术语表，.scratch/ 是规格与工单
```

## 开发

- 测试：`cargo test --all --workspace`（纯函数、应用层用脚本化的 fake gateway，界面用
  `gpui_kit::test` 无头渲染）。
- 术语以 [CONTEXT.md](CONTEXT.md) 为准；重要取舍写进 [docs/adr](docs/adr)。
- 规格与工单在 `.scratch/`；接手某条线之前先读对应的 `spec.md`。

## 已知边界

- 通道 tag 指向哪个提交，看板上不显示（只显示各目标的 job 状态）。
- 下载没有流式接口：产物整个读进内存再落盘，所以给了进度条与取消，但大文件仍是内存里走一遭。
- 本地通道指针（ADR-0005 那套）在代码里还留着，界面已经不用它（改用 tag 之后正在清理）。
- 窗口里不再显示应用自身的版本 / 目标 / 打包配置，用 `--build-info` 或启动日志查看。
