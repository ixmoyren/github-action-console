//! 构建期把"我是谁"写进二进制：版本号来自包版本，构建目标来自 target triple，
//! 打包配置来自 CI 注入的环境变量（缺失时程序显示"未指定"）。

fn main() {
    let target = std::env::var("TARGET").expect("TARGET is set by cargo for build scripts");
    println!("cargo:rustc-env=GAC_CLIENT_TARGET={target}");

    println!("cargo:rerun-if-env-changed=GAC_PACKAGING_CONFIG");
    if let Ok(config) = std::env::var("GAC_PACKAGING_CONFIG") {
        println!("cargo:rustc-env=GAC_PACKAGING_CONFIG={config}");
    }
}
