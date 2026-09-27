# 06: 运行详情与 job 日志

**What to build:** 打开一次 workflow run，看到各个 job 与其中 step 的状态，从而定位是哪一阶段出的问题；可以直接读取某个 job 的原始日志文本，支持滚动、搜索与复制，不必先下载再解压；并能一键在浏览器里打开这次运行对应的 GitHub 页面。

**Blocked by:** 05（工作流与运行列表）

**Status:** ready-for-agent

- [ ] 运行详情显示各 job 与 step 的状态
- [ ] 可直接读取某个 job 的原始日志文本，支持滚动、搜索、复制
- [ ] 一键在浏览器打开该 run 的 GitHub 页面
- [ ] job/step 状态映射与日志读取由 fake gateway 测试，覆盖失败 job 与空日志

## Comments

**2026-09-27 重做 jobs 区**：原来 job 卡片就是一行摘要加一句 `步骤：1 名称 结论 / 2 …`，
而且整页不滚——抽屉高度固定，内容一多就被裁掉。现在整个运行详情页装进 gpui-kit 的可滚动
容器（`.overflow_y_scrollbar()`，`flex_1 + min_h_0` 让抽屉的高度兜住它），每个 job 的步骤
各用一个 `Editor`（只读、等宽、带行号，能选中能复制），高度按步数算足（gpui-component 的
编辑器不会自己长高，不给高度只露一行），看不全的交给整页滚动。编辑器按 job id 建与收，
和 workflow 文件那份一样在 render 里对账（`sync_job_editors`）。
覆盖测试：`the_jobs_page_scrolls_and_shows_each_jobs_steps_in_an_editor`。
