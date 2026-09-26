pub const BUILD_TARGET: &str = env!("GAC_BUILD_TARGET");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppInfo {
    version: String,
    build_target: String,
    packaging_config: Option<String>,
}

impl AppInfo {
    pub fn new(
        version: impl Into<String>,
        build_target: impl Into<String>,
        packaging_config: Option<&str>,
    ) -> Self {
        Self {
            version: version.into(),
            build_target: build_target.into(),
            packaging_config: normalize_packaging_config(packaging_config),
        }
    }

    pub fn from_build() -> Self {
        Self::new(
            env!("CARGO_PKG_VERSION"),
            BUILD_TARGET,
            option_env!("GAC_PACKAGING_CONFIG"),
        )
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn build_target(&self) -> &str {
        &self.build_target
    }

    pub fn packaging_config(&self) -> Option<&str> {
        self.packaging_config.as_deref()
    }

    /// 这个二进制是谁：版本号、构建目标、打包配置。
    ///
    /// `--build-info` 与 CI 的自检都读这份文本，用来在没有窗口的地方确认"手里这个
    /// 产物是哪个目标、哪份配置产出的"。三者缺一不可，所以打包配置缺失时写"未指定"
    /// 而不是留空。
    pub fn report(&self) -> String {
        format!(
            "GitHub Action Console\n版本号：{}\n构建目标：{}\n打包配置：{}\n",
            self.version,
            self.build_target,
            self.packaging_config().unwrap_or("未指定")
        )
    }
}

pub fn normalize_packaging_config(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_packaging_config_is_none() {
        assert_eq!(normalize_packaging_config(None), None);
    }

    #[test]
    fn blank_packaging_config_is_none() {
        assert_eq!(normalize_packaging_config(Some("")), None);
        assert_eq!(normalize_packaging_config(Some("   ")), None);
    }

    #[test]
    fn packaging_config_is_trimmed() {
        assert_eq!(
            normalize_packaging_config(Some("  macos-mas  ")).as_deref(),
            Some("macos-mas")
        );
    }

    #[test]
    fn app_info_keeps_provided_values() {
        let info = AppInfo::new("0.1.0", "x86_64-unknown-linux-gnu", Some(" macos-mas "));
        assert_eq!(info.version(), "0.1.0");
        assert_eq!(info.build_target(), "x86_64-unknown-linux-gnu");
        assert_eq!(info.packaging_config(), Some("macos-mas"));
    }

    #[test]
    fn app_info_reports_missing_packaging_config_as_none() {
        let info = AppInfo::new("0.1.0", "x86_64-unknown-linux-gnu", None);
        assert_eq!(info.packaging_config(), None);
    }

    #[test]
    fn app_info_from_build_always_has_version_and_target() {
        let info = AppInfo::from_build();
        assert!(!info.version().is_empty());
        assert!(!info.build_target().is_empty());
    }

    #[test]
    fn the_report_names_all_three_facts() {
        let report = AppInfo::new("0.1.0", "aarch64-apple-darwin", Some("macos-dmg")).report();
        assert!(report.contains("0.1.0"), "{report}");
        assert!(report.contains("aarch64-apple-darwin"), "{report}");
        assert!(report.contains("macos-dmg"), "{report}");

        // 打包配置没传时也要照样报，写"未指定"而不是空着。
        let bare = AppInfo::new("0.1.0", "x86_64-unknown-linux-gnu", None).report();
        assert!(bare.contains("未指定"), "{bare}");
    }
}
