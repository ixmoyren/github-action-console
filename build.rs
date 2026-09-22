fn main() {
    println!(
        "cargo:rustc-env=GAC_BUILD_TARGET={}",
        std::env::var("TARGET").expect("TARGET is set by cargo for build scripts")
    );
    println!("cargo:rerun-if-env-changed=GAC_PACKAGING_CONFIG");
    if let Ok(value) = std::env::var("GAC_PACKAGING_CONFIG") {
        println!("cargo:rustc-env=GAC_PACKAGING_CONFIG={value}");
    }
}
