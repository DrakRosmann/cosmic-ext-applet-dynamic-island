// SPDX-License-Identifier: GPL-3.0-only

mod app;
mod i18n;
mod media;
mod power;
mod timer;
mod widgets;

pub const APP_ID: &str = "io.github.DrakRosmann.CosmicExtAppletDynamicIsland";

fn main() -> cosmic::iced::Result {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                // While the popup redraws, iced warns of each event one of
                // libcosmic's subscriptions doesn't take: dozens a second.
                .unwrap_or_else(|_| {
                    "warn,iced_futures::subscription::tracker=error,cosmic_ext_applet_dynamic_island=info"
                        .into()
                }),
        )
        .init();
    cosmic::applet::run::<app::App>(())
}
