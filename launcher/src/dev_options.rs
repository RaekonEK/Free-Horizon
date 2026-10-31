//! Offline play: Settings' Account row that lets offline names — and the game's own name — play
//! without a Microsoft account that owns the game (`backend::Settings::dev_offline`). On by
//! default: signing in with Microsoft is optional, not a requirement. The game still comes from
//! the player's own install: the launcher downloads Mojang's files only for an owner
//! (`backend::download_for_owner`).
use super::*;

impl Launcher {
    /// Settings' offline-play row, in the Account group.
    pub(super) fn offline_play_row(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let toggle = switch("offline-play", self.settings.offline_play(), &t).on_click(
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.settings.dev_offline = Some(!this.settings.offline_play());
                this.settings.save();
                cx.notify();
            }),
        );
        index_row(
            0,
            tr("Play offline (no Microsoft account)"),
            Some(
                div()
                    .child(tr(
                        "Offline names, and the game's own name, play without signing in. Signing \
                         in with Microsoft is optional; online servers still need an account. \
                         Minecraft itself still comes from your own install.",
                    ))
                    .into_any_element(),
            ),
            Some(toggle.into_any_element()),
            &t,
        )
    }
}
