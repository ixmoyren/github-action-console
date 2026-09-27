# Github Action Console

一个通用型桌面控制台：登录 GitHub 后选择仓库，触发、观察并推进软件发布，并管理各通道当前指向的发布版本。

## Language

### 发布模型

**发布版本 (Release version)**:
由 git tag 标识的版本身份，不要求对应的 GitHub Release 已存在。 _Avoid_: 版本号, release, build

**通道 (Channel)**:
版本推进的轨道（LTS / latest / dogfood），在仓库里就是同名 git tag：tag 指向哪个提交，
这条通道现在就是哪次构建。移动 tag 是正常用法。 _Avoid_: 轨道, 分发渠道, 分支

**通道 tag (Channel tag)**:
见「通道」——`lts` / `latest` / `dogfood` 本身就是 tag，控制台不另存一份指针。 _Avoid_: 通道指针

**分发目标 (Distribution target)**:
产物以什么形式交付：GitHub Releases、Mac App Store、直接下载。 _Avoid_: 分发渠道, 渠道

**发布目标 (Release target)**:
平台 × 架构 × 分发目标的组合（如 `macos-arm64-github`、`macos-x64-mas`）。 _Avoid_: 构建目标, target, 分发目标

**MAS**:
经 Mac App Store 分发的 macOS 发布目标。未签名包的构建可以由 CI 完成，签名、公证与上架属于商店侧。 _Avoid_: 商店版, App Store 版

**web-arm 与 web-intel**:
经官网分发的 macOS 发布目标，分别是 Apple Silicon 与 Intel 架构的客户端；不是网页应用。 _Avoid_: 网页版, web 应用, 在线版

**Windows 目标**:
经官网分发的 Windows 客户端发布目标。 _Avoid_: PC 版, Windows 应用

**打包配置 (Packaging config)**:
可复用的构建配方，记录要触发的 workflow 标识与 inputs 模板。 _Avoid_: 构建配置, 打包脚本, 配方

**发布清单 (Release manifest)**:
仓库内的一份 YAML 文件，是发布目标与打包配置定义的真相源。 _Avoid_: 配置, release config

**发布记录 (Release record)**:
一个发布目标在一个发布版本上的发布事实：该版本带着这个目标的发布资产，且这个目标的通道指针指向它。 _Avoid_: 发布历史, 记录表

### 构建与产物

**构建 (Build)**:
一次由 workflow_dispatch 触发的 workflow run，用于产出某个发布目标的产物。 _Avoid_: 编译, job, task

**构建产物 (Build artifact)**:
一次构建产出的 GitHub Actions artifact，临时且会过期。 _Avoid_: 产物, artifact, asset

**发布资产 (Release asset)**:
附加在 GitHub Release 上的文件，拥有公开的下载地址。 _Avoid_: 产物, artifact, 构建产物

### 真实与模拟

**构建对象 (Build subject)**:
被这套发布链路打包、分发的那个客户端。本项目里就是本应用自身（`github-action-console`）：它报得出自己的版本号、构建目标与打包配置，`--build-info` 在无头环境里报同一份事实。 _Avoid_: 示例客户端, demo, 测试程序

**真实链路 (Real path)**:
在 CI runner 上真正执行、真正产出产物的步骤。 _Avoid_: 正式流程

**模拟 (Simulated)**:
明确标注为未真实执行的步骤：签名、公证、商店提交，以及缺少 runner 的平台。模拟步骤不代表任何已完成的事实。 _Avoid_: 假的, mock, 占位

### 界面

**发布看板 (Release board)**:
看板视图；卡片是一个发布目标在一个发布版本下的发布进程，列是该进程的发布状态。 _Avoid_: 看板, kanban

**发布状态 (Release state)**:
发布进程的状态机取值：待构建 / 构建中 / 构建成功 / 待签名公证 / 待发布 / 已发布 / 失败 / 已取消。 _Avoid_: 状态, status
