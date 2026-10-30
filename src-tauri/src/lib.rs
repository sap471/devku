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
pub mod session;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let paths = paths::Paths::resolve();
    let session = session::Session::new(paths, None);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppState::new(session))
        .invoke_handler(tauri::generate_handler![
            commands::snapshot,
            commands::start,
            commands::stop,
            commands::add_site,
            commands::remove_site,
            commands::set_site_enabled,
            commands::detect_project,
            commands::tail_log,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
