use super::*;

use gpui_kit::component::group_box::GroupBox;

/// The settings child window. It owns its own proxy field so the parent view's
/// input keeps whatever the user was typing there.
pub(super) struct SettingsWindow {
    proxy_input: Entity<InputState>,
    store: Store,
    gateway: Arc<dyn GitHubGateway>,
    runtime: TokioRuntime,
}

impl SettingsWindow {
    pub(super) fn new(
        store: Store,
        gateway: Arc<dyn GitHubGateway>,
        runtime: TokioRuntime,
        initial_proxy: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let proxy_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder(labels::SETTINGS_PROXY_PLACEHOLDER, window, cx);
            if let Some(proxy) = initial_proxy {
                state.set_value(proxy, window, cx);
            }
            state
        });

        Self {
            proxy_input,
            store,
            gateway,
            runtime,
        }
    }

    /// Persist every setting, then close this window.
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let raw = self.proxy_input.read(cx).value().to_string();
        let store = self.store.clone();
        let gateway = self.gateway.clone();
        let runtime = self.runtime.clone();
        let handle = window.window_handle();
        info!(proxy = %raw.trim(), "saving the settings");

        cx.spawn(async move |_this, cx| {
            let task = runtime.spawn(async move {
                let cleaned = raw.trim().to_owned();
                let value = if cleaned.is_empty() {
                    None
                } else {
                    Some(cleaned)
                };
                if let Err(error) = store.save_proxy(value.as_deref()).await {
                    warn!(%error, "could not persist the proxy setting");
                }
                gateway.set_proxy(value);
            });
            if let Err(error) = task.await {
                warn!(%error, "the settings save task did not finish");
            }

            match handle.update(cx, |_, window, _| window.remove_window()) {
                Ok(()) => info!("settings saved; the settings window closed"),
                Err(error) => warn!(%error, "could not close the settings window"),
            }
        })
        .detach();
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        reset_pickable_ids();

        // Self-drawn title bar: the window is borderless.
        let title_bar = div()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .p_3()
            .child(pickable(labels::SETTINGS_TITLE));

        // Themed settings section, the component's group-box look.
        let group = GroupBox::new()
            .id("network-settings")
            .title(pickable(labels::SETTINGS_GROUP_TITLE))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .w_full()
                    .child(pickable(labels::SETTINGS_PROXY_LABEL))
                    .child(Input::new(&self.proxy_input).flex_1()),
            );

        let footer = div().flex().flex_row().justify_end().w_full().p_3().child(
            Button::new("save-settings")
                .label(labels::SETTINGS_SAVE)
                .primary()
                .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(title_bar)
            .child(div().flex_1().w_full().p_3().child(group))
            .child(footer)
            .into_any_element()
    }
}
