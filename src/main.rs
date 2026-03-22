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

    iced::application("XMPP Messenger", ui::App::update, ui::App::view)
        .subscription(|_state| ui::App::subscription())
        .run_with(ui::App::new)
}
