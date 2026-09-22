use gpui_kit::component::{Root, Theme, label::Label};
use gpui_kit::*;

use crate::app_info::AppInfo;
use crate::strings;

struct AppShell {
    info: AppInfo,
}

impl AppShell {
    fn new(info: AppInfo) -> Self {
        Self { info }
    }

    fn info_row(label: &'static str, value: &str) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .child(Label::new(label).text_sm())
            .child(Label::new(value.to_owned()))
    }
}

impl Render for AppShell {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let packaging_config = self.info.packaging_config().unwrap_or(strings::UNSPECIFIED);

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .child(Self::info_row(strings::LABEL_VERSION, self.info.version()))
            .child(Self::info_row(
                strings::LABEL_BUILD_TARGET,
                self.info.build_target(),
            ))
            .child(Self::info_row(
                strings::LABEL_PACKAGING_CONFIG,
                packaging_config,
            ))
    }
}

pub fn run() {
    let info = AppInfo::from_build();

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some(strings::APP_TITLE.into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    Theme::sync_system_appearance(Some(window), cx);
                    let view = cx.new(|_| AppShell::new(info.clone()));
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("failed to open window");
        })
        .detach();
    });
}
