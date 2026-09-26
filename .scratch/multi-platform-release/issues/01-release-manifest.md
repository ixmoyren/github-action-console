# 01: 发布清单：解析与校验

**What to build:** 读仓库里的 `.github/release-console.yml`，得到一个仓库的发布目标（平台 × 架构 × 分发方式 + 用哪份打包配置）与打包配置（workflow 标识 + inputs 模板）。清单是定义的真相源（ADR-0003），所以解析必须给出可用的错误信息：缺字段、引用了不存在的打包配置、目标名重复或非法都要挡住。清单里不读指针。

**Status:** ready-for-agent

- [ ] 解析 `targets.<name>`：`platform`、`arch`、`distribution`、`packaging`，可选 `simulated`
- [ ] 解析 `packaging.<name>`：`workflow` 与 `inputs` 模板
- [ ] 校验：目标引用的打包配置必须存在；缺字段、空名、非法字符给出明确错误
- [ ] 缺清单时给出"该仓库还没有发布清单"这一独立状态，不是解析失败
- [ ] 解析结果可被触发表单与看板直接使用（发布目标列表 + 每目标的打包配置）
- [ ] 单元测试：合法清单、缺字段、悬空引用、空清单各一例
