mod config;
mod store;
mod ui;
mod xmpp;

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("xmpp_start=debug".parse().unwrap()),
        )
        .init();

    // Load settings synchronously at startup (no async needed — it's just fs::read).
    let settings = config::load();

    iced::application("XMPP Messenger", ui::App::update, ui::App::view)
        .subscription(|_state| ui::App::subscription())
        .theme(|app| app.iced_theme())
        .run_with(move || ui::App::new_with_settings(settings))
}
