pub mod api;
pub mod archive;
pub mod autostart;
pub mod caddy;
pub mod cli;
pub mod commands;
pub mod composer;
pub mod config;
pub mod console;
pub mod detect;
pub mod download;
pub mod hosts;
pub mod launcher;
pub mod path_env;
pub mod paths;
pub mod php;
pub mod ports;
pub mod proc;
pub mod runtime;
pub mod runtime_detect;
pub mod scripts;
pub mod session;
pub mod subprocess;
pub mod terminal;
pub mod tray;

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
        .manage(download::Slot::default())
        // Tutup jendela hanya menyembunyikan; stop beneran lewat tray > Keluar.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "main"
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        // Service menyala sendiri: membuka DevKu berarti localhost langsung hidup.
        .setup(|app| {
            tray::create(app.handle())?;

            // Dibuka oleh Windows saat login: jangan tampilkan jendela, tapi
            // service tetap dinyalakan supaya tray langsung berguna. Jendela
            // dibuat tidak terlihat di config, lalu ditampilkan di sini untuk
            // pemakaian biasa.
            let hidden = std::env::args().any(|arg| arg == autostart::HIDDEN_FLAG);
            if let Some(window) = app.get_webview_window("main") {
                let _ = if hidden { window.hide() } else { window.show() };
            }

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<commands::AppState>();
                let mut guard = state.session.lock().await;
                if let Some(session) = guard.as_mut()
                    && (hidden || session.config().autostart)
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
            commands::trust_caddy,
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
            commands::php_catalog,
            commands::install_php,
            commands::cancel_download,
            commands::set_site_php,
            commands::set_default_php,
            commands::enable_php_path,
            commands::disable_php_path,
            commands::install_caddy,
            commands::install_bun,
            commands::install_composer,
            commands::finish_onboarding,
            commands::export_config,
            commands::import_config,
            commands::set_start_at_login,
            commands::enable_composer_global_path,
            commands::disable_composer_global_path,
            commands::site_commands,
            commands::open_targets,
            commands::open_in,
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
