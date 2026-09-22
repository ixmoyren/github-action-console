use super::*;
use super::{AppView, Services};

pub(super) fn default_download_dir() -> std::io::Result<std::path::PathBuf> {
    let database = default_store_path()?;
    let dir = database
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("downloads");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub(super) fn default_store_path() -> std::io::Result<std::path::PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(std::path::PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(|home| std::path::PathBuf::from(home).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| std::path::PathBuf::from(home).join(".local/share"))
            })
    };

    let dir = base
        .unwrap_or_else(std::env::temp_dir)
        .join("github-action-console");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("console.sqlite"))
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let info = AppInfo::from_build();
    let runtime = TokioRuntime::new()?;
    let store = runtime.block_on(Store::open(default_store_path()?))?;
    let gateway: Arc<dyn GitHubGateway> = Arc::new(OctocrabGateway::new(
        option_env!("GITHUB_CLIENT_ID").map(str::to_owned),
    ));
    let manager = Arc::new(Mutex::new(AuthManager::new(gateway.clone(), store.clone())));
    let picker = Arc::new(Mutex::new(RepositoryList::new(gateway.clone(), store)));
    let workspace = Arc::new(Mutex::new(Workspace::new()));
    let detail = Arc::new(Mutex::new(RunDetail::new()));
    let downloads = Arc::new(Mutex::new(Downloads::new(default_download_dir()?)));
    let status = Arc::new(Mutex::new(Status::new()));

    application().run(move |cx| {
        init(cx);
        let info = info.clone();
        let gateway = gateway.clone();
        let manager = manager.clone();
        let picker = picker.clone();
        let workspace = workspace.clone();
        let detail = detail.clone();
        let downloads = downloads.clone();
        let status = status.clone();
        let runtime = runtime.clone();

        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some(labels::APP_TITLE.into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    Theme::sync_system_appearance(Some(window), cx);
                    let services = Services {
                        gateway,
                        manager,
                        picker,
                        workspace,
                        detail,
                        downloads,
                        status,
                        runtime,
                    };
                    let view = cx.new(|cx| AppView::new(info, services, window, cx));
                    view.update(cx, |this, cx| {
                        this.wire(cx);
                        this.restore(cx);
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("failed to open window");
        })
        .detach();
    });

    Ok(())
}
