# 04: CI 模板：按发布目标的真实构建链路

**What to build:** `.github/workflows/release-target.yml`：`workflow_dispatch`（inputs：target / version / config）与 `push: tags: ['v*']` 双触发；`plan` 读清单产出目标矩阵；`build` 按目标真实构建并打包（web-arm dmg、web-intel dmg、windows msi、mas 未签名 pkg、linux-tar tar.gz），上传构建产物，模拟步骤用 `::notice::`/`::warning::` 标注；`release` 在 tag 触发时把产物挂到该版本的 GitHub Release。

**Blocked by:** 03（示例客户端）

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

草案已写好并本地校验过 YAML：`plan`（读清单出矩阵与版本）+ `build`（按目标独立 job，fail-fast: false，dmg / 未签名 pkg / WiX msi / tar.gz，upload-artifact，模拟步骤用 ::warning:: 标注）+ `release`（tag 触发时 gh release create/upload）。 **仍缺一次真实运行**：需要推到有 Actions 的仓库跑一遍并留存 run 链接。
