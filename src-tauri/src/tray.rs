use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::commands::AppState;

/// Tray wajib ada supaya buka-tutup jendela tidak lagi sama dengan mematikan
/// localhost: menutup jendela hanya menyembunyikannya, service tetap jalan.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Buka DevKu", true, None::<&str>)?;
    let start = MenuItem::with_id(app, "start", "Start semua", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", "Stop semua", true, None::<&str>)?;
    let browser = MenuItem::with_id(app, "browser", "Buka browser", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Keluar", true, None::<&str>)?;

    let menu = MenuBuilder::new(app)
        .item(&open)
        .separator()
        .item(&start)
        .item(&stop)
        .separator()
        .item(&browser)
        .separator()
        .item(&quit)
        .build()?;

    TrayIconBuilder::with_id("devku")
        .icon(app.default_window_icon().expect("ikon aplikasi").clone())
        .tooltip("DevKu")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            // Klik kiri tanpa menu: langsung tampilkan jendela, seperti umumnya
            // aplikasi tray di Windows.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| handle(app, event.id().as_ref()))
        .build(app)?;

    Ok(())
}

fn handle(app: &AppHandle, id: &str) {
    match id {
        "open" => show_main(app),
        "start" => spawn_toggle(app.clone(), true),
        "stop" => spawn_toggle(app.clone(), false),
        "browser" => {
            let _ = tauri_plugin_opener::open_url("http://localhost/", None::<&str>);
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

fn spawn_toggle(app: AppHandle, start: bool) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let mut guard = state.session.lock().await;
        if let Some(session) = guard.as_mut() {
            let result = if start {
                session.start().await
            } else {
                session.stop().await;
                Ok(())
            };
            if let Err(err) = result {
                eprintln!(
                    "tray {id}: {err:#}",
                    id = if start { "start" } else { "stop" }
                );
            }
        }
    });
}
