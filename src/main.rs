fn main() {
    if let Err(error) = github_action_console::ui::run() {
        eprintln!("failed to start GitHub Action Console: {error}");
        std::process::exit(1);
    }
}
