# 示例客户端（demo client）

发布控制台的 **构建对象**：一个只显示版本号、构建目标与打包配置的程序。它不实现任何业务功能，存在的意义是被打包成 dmg / msi / pkg / tar.gz，用来打通"选输入 → 触发 runner → 构建 → 产物 → 发布记录"这条真实链路。

## 构建

```sh
# 本机构建（本机 target）
cargo run --manifest-path demo-client/Cargo.toml

# 指定打包配置（CI 就是这么做的）
GAC_PACKAGING_CONFIG=macos-dmg \
  cargo build --release --manifest-path demo-client/Cargo.toml

# 交叉构建（在 Apple Silicon runner 上产 Intel 包）
rustup target add x86_64-apple-darwin
cargo build --release --manifest-path demo-client/Cargo.toml --target x86_64-apple-darwin
```

三者由 `build.rs` 在构建期注入：版本号取 `CARGO_PKG_VERSION`，构建目标取 `TARGET`，打包配置取 `GAC_PACKAGING_CONFIG`（未指定时显示"未指定"）。

## 打包

`.github/workflows/release-target.yml` 按发布目标打包，产物形式由 `.github/release-console.yml` 里该目标的打包配置决定：

| 目标        | 产物                                         | 真实 / 模拟                                 |
|-------------|----------------------------------------------|---------------------------------------------|
| `web-arm`   | `gac-demo-client-<version>-web-arm.dmg`      | 全部真实（hdiutil）                         |
| `web-intel` | `gac-demo-client-<version>-web-intel.dmg`    | 全部真实（交叉构建 + hdiutil）              |
| `windows`   | `gac-demo-client-<version>-windows.msi`      | WiX 打包真实；**代码签名模拟**              |
| `mas`       | `gac-demo-client-<version>-mas-unsigned.pkg` | 未签名 pkg 真实；**签名 / 公证 / 上架模拟** |
| `linux-tar` | `gac-demo-client-<version>-linux-tar.tar.gz` | 演示自加目标，全部真实                      |

Windows 的 WiX 定义见 `packaging/windows/main.wxs`（草案，变量由 CI 传入）。
