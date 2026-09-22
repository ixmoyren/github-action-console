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
    let base = dirs::data_dir().unwrap_or_else(std::env::temp_dir);

    let dir = base.join("github-action-console");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("console.sqlite"))
}

/// 60% of the primary display's width by 40% of its height, centred on it.
fn centered_window_bounds(cx: &AsyncApp) -> WindowBounds {
    const WIDTH_FRACTION: f32 = 0.6;
    const HEIGHT_FRACTION: f32 = 0.4;

    let fallback = || {
        WindowBounds::Windowed(Bounds::new(
            Point {
                x: px(0.0),
                y: px(0.0),
            },
            Size {
                width: px(1200.0),
                height: px(760.0),
            },
        ))
    };

    let Some(display) = cx.update(|cx| cx.primary_display()) else {
        return fallback();
    };

    let display_bounds = display.bounds();
    let width = display_bounds.size.width * WIDTH_FRACTION;
    let height = display_bounds.size.height * HEIGHT_FRACTION;
    let origin = Point {
        x: display_bounds.origin.x + (display_bounds.size.width - width) / 2.0,
        y: display_bounds.origin.y + (display_bounds.size.height - height) / 2.0,
    };

    WindowBounds::Windowed(Bounds::new(origin, Size { width, height }))
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let info = AppInfo::from_build();
    let runtime = TokioRuntime::new()?;
    let store = runtime.block_on(Store::open(default_store_path()?))?;
    let gateway: Arc<dyn GitHubGateway> = Arc::new(OctocrabGateway::new());
    let initial_proxy = runtime.block_on(store.load_proxy()).ok().flatten();
    if let Some(proxy) = &initial_proxy {
        gateway.set_proxy(Some(proxy.clone()));
    }
    let manager = Arc::new(Mutex::new(AuthManager::new(gateway.clone(), store.clone())));
    let picker = Arc::new(Mutex::new(RepositoryList::new(
        gateway.clone(),
        store.clone(),
    )));
    let workspace = Arc::new(Mutex::new(Workspace::new()));
    let detail = Arc::new(Mutex::new(RunDetail::new()));
    let downloads = Arc::new(Mutex::new(Downloads::new(default_download_dir()?)));
    let status = Arc::new(Mutex::new(Status::new()));

    application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
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
                let window_bounds = centered_window_bounds(cx);
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(window_bounds),
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
                            store,
                            initial_proxy,
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
