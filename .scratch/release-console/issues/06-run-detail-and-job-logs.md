# 06: 运行详情与 job 日志

**What to build:** 打开一次 workflow run，看到各个 job 与其中 step 的状态，从而定位是哪一阶段出的问题；可以直接读取某个 job 的原始日志文本，支持滚动、搜索与复制，不必先下载再解压；并能一键在浏览器里打开这次运行对应的 GitHub 页面。

**Blocked by:** 05（工作流与运行列表）

**Status:** ready-for-agent

- [ ] 运行详情显示各 job 与 step 的状态
- [ ] 可直接读取某个 job 的原始日志文本，支持滚动、搜索、复制
- [ ] 一键在浏览器打开该 run 的 GitHub 页面
- [ ] job/step 状态映射与日志读取由 fake gateway 测试，覆盖失败 job 与空日志
