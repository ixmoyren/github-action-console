# Github Action Console

一个通用型桌面控制台：登录 GitHub 后选择仓库，触发、观察并推进软件发布，并管理各通道当前指向的发布版本。

## Language

### 发布模型

**发布版本 (Release version)**:
由 git tag 标识的版本身份，不要求对应的 GitHub Release 已存在。 _Avoid_: 版本号, release, build

**通道 (Channel)**:
版本推进的轨道（stable / beta / nightly），当前指向一个发布版本。 _Avoid_: 轨道, 分发渠道, 分支

**通道指针 (Channel pointer)**:
通道到它所指向的发布版本的存储映射。 _Avoid_: 当前版本, latest

**分发目标 (Distribution target)**:
产物以什么形式交付：GitHub Releases、Mac App Store、直接下载。 _Avoid_: 分发渠道, 渠道

**发布目标 (Release target)**:
平台 × 架构 × 分发目标的组合（如 `macos-arm64-github`、`macos-x64-mas`）。 _Avoid_: 构建目标, target, 分发目标

**打包配置 (Packaging config)**:
可复用的构建配方，记录要触发的 workflow 标识与 inputs 模板。 _Avoid_: 构建配置, 打包脚本, 配方

**发布清单 (Release manifest)**:
仓库内的一份 YAML 文件，是发布目标与打包配置定义的真相源。 _Avoid_: 配置, release config

### 构建与产物

**构建 (Build)**:
一次由 workflow_dispatch 触发的 workflow run，用于产出某个发布目标的产物。 _Avoid_: 编译, job, task

**构建产物 (Build artifact)**:
一次构建产出的 GitHub Actions artifact，临时且会过期。 _Avoid_: 产物, artifact, asset

**发布资产 (Release asset)**:
附加在 GitHub Release 上的文件，拥有公开的下载地址。 _Avoid_: 产物, artifact, 构建产物

### 界面

**发布看板 (Release board)**:
看板视图；卡片是一个发布目标在一个发布版本下的发布进程，列是该进程的发布状态。 _Avoid_: 看板, kanban

**发布状态 (Release state)**:
发布进程的状态机取值：待构建 / 构建中 / 构建成功 / 待签名公证 / 待发布 / 已发布 / 失败 / 已取消。 _Avoid_: 状态, status
