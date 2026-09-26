//! 示例客户端：一个只为打通发布链路而存在的程序。
//!
//! 它只做一件事——显示自己是谁：版本号、构建目标、打包配置。打包成 dmg / msi /
//! pkg / tar.gz 之后，用它验证"手里这个包是哪个目标、哪份配置产出的"。

const VERSION: &str = env!("CARGO_PKG_VERSION");
const BUILD_TARGET: &str = env!("GAC_CLIENT_TARGET");
const PACKAGING_CONFIG: &str = match option_env!("GAC_PACKAGING_CONFIG") {
    Some(config) => config,
    None => "未指定",
};

fn main() {
    println!("GitHub Action Console 示例客户端");
    println!("版本号：{VERSION}");
    println!("构建目标：{BUILD_TARGET}");
    println!("打包配置：{PACKAGING_CONFIG}");
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_knows_its_version_and_target() {
        assert!(!VERSION.is_empty());
        assert!(BUILD_TARGET.contains('-'), "{BUILD_TARGET}");
    }
}
