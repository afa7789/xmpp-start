mod config;
pub mod i18n;
pub mod notifications;
mod store;
mod ui;
mod xmpp;

use std::sync::Arc;

fn main() -> iced::Result {
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("xmpp_start=debug".parse().unwrap()),
        )
        .init();

    // Load settings synchronously at startup (no async needed — it's just fs::read).
    let settings = config::load();

    // Open the SQLite database synchronously before starting the iced event loop.
    // This ensures the pool is available as soon as the first XMPP event arrives.
    let pool = {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime for DB init");
        let path = config::db_path();
        tracing::info!("opening database at {path}");
        Arc::new(
            rt.block_on(store::Database::connect(&path))
                .expect("failed to open database")
                .pool,
        )
    };

    iced::application("XMPP Messenger", ui::App::update, ui::App::view)
        .subscription(|state: &ui::App| state.subscription())
        .theme(ui::App::iced_theme)
        .run_with(move || ui::App::new_with_settings(settings, pool))
}
