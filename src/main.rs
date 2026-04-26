#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod core;
mod drop_resolve;
mod game_store;
mod settings;

fn main() -> iced::Result {
    let app_dir = core::app_data_dir();
    let settings = settings::Settings::load_or_create(&app_dir);

    // Production-style: always GUI subsystem (no console), logs go to file.
    // Runtime `mode` switches verbosity, not cargo build profile.
    init_logging(&app_dir, settings.mode);

    app::run(settings)
}

fn init_logging(app_dir: &std::path::Path, mode: settings::AppMode) {
    use tracing_subscriber::EnvFilter;

    let filter = match mode {
        settings::AppMode::Debug => "debug",
        settings::AppMode::Release => "info",
    };

    let log_path = app_dir.join("launcher.log");
    let writer = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path);

    if let Ok(file) = writer {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new(filter))
            .with_writer(std::sync::Mutex::new(file))
            .with_ansi(false)
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new(filter))
            .with_ansi(false)
            .try_init();
    }
}
