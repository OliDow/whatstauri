mod badge;
mod config;
mod last_message;
mod links;
mod notifications;
mod tray;
mod unread;
mod window;
mod wipe;

use std::sync::Mutex;

pub fn run() {
    let cfg = config::Config::load();
    cfg.apply_env_workarounds(); // must precede GTK/WebKit init

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            notifications::focus_main(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            notifications::deliver_notification,
            notifications::report_title,
            notifications::debug_log
        ])
        .manage(notifications::UnreadState(Mutex::new(0)))
        .manage(cfg.clone())
        .setup(move |app| {
            wipe::handle_pending_wipe(app.handle()); // BEFORE the webview opens its storage
            tray::create(app.handle())?;
            window::build_main_window(app.handle(), &cfg)?;
            tray::first_run_gnome_hint(app.handle());
            Ok(())
        })
        .on_window_event(|win, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                use tauri::Manager;
                let cfg = win.app_handle().state::<config::Config>();
                if cfg.close_to_tray {
                    let _ = win.hide();
                    api.prevent_close();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running whatstauri");
}
