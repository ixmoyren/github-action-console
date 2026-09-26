fn main() {
    // `--build-info` 只回答"这个二进制是谁"：窗口不必起来，CI 的打包自检读它。
    if std::env::args().skip(1).any(|arg| arg == "--build-info") {
        print!(
            "{}",
            github_action_console::app_info::AppInfo::from_build().report()
        );
        return;
    }

    if let Err(error) = github_action_console::ui::run() {
        eprintln!("failed to start GitHub Action Console: {error}");
        std::process::exit(1);
    }
}
