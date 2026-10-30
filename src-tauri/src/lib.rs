pub mod api;
pub mod caddy;
pub mod commands;
pub mod config;
pub mod detect;
pub mod hosts;
pub mod paths;
pub mod php;
pub mod ports;
pub mod proc;
pub mod runtime;
pub mod runtime_detect;
pub mod session;
pub mod terminal;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tauri::Manager;

    let paths = paths::Paths::resolve();
    let session = session::Session::new(paths, None);

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppState::new(session))
        .manage(terminal::Terminals::default())
        // Service menyala sendiri: membuka DevKu berarti localhost langsung hidup.
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<commands::AppState>();
                let mut guard = state.session.lock().await;
                if let Some(session) = guard.as_mut()
                    && session.config().autostart
                {
                    let _ = session.start().await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::snapshot,
            commands::start,
            commands::stop,
            commands::add_site,
            commands::remove_site,
            commands::set_site_enabled,
            commands::detect_project,
            commands::tail_log,
            commands::runtimes,
            commands::open_terminal,
            commands::write_terminal,
            commands::resize_terminal,
            commands::close_terminal,
        ])
        .build(tauri::generate_context!())
        .expect("error while building DevKu");

    app.run(|handle, event| {
        // Tanpa ini, menutup jendela meninggalkan php-cgi yang memegang port.
        if let tauri::RunEvent::ExitRequested { .. } = event {
            let state = handle.state::<commands::AppState>();
            tauri::async_runtime::block_on(async {
                if let Ok(mut guard) = state.session.try_lock()
                    && let Some(session) = guard.as_mut()
                {
                    session.stop().await;
                }
            });
        }
    });
}
