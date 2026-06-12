use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

/// Menu items we update after creation. The Show/Hide label mirrors the unread
/// count because tray-icon's GTK backend makes tooltips a no-op on Linux.
pub struct TrayMenuItems {
    pub show_hide: MenuItem<tauri::Wry>,
}

fn show_hide_label(count: u32) -> String {
    if count == 0 {
        "Show/Hide WhatsApp".to_string()
    } else {
        format!("Show/Hide WhatsApp ({count} unread)")
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let show_hide = MenuItem::with_id(app, "show_hide", "Show/Hide WhatsApp", true, None::<&str>)?;
    let reload = MenuItem::with_id(app, "reload", "Reload", true, None::<&str>)?;
    let clear = MenuItem::with_id(
        app,
        "clear_data",
        "Clear data & re-login…",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_hide, &reload, &clear, &quit])?;

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().expect("bundled icon").clone())
        .menu(&menu)
        .tooltip("WhatsTauri")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show_hide" => toggle_main(app),
            "reload" => crate::window::reload_main(app),
            "clear_data" => crate::wipe::request_wipe(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    app.manage(TrayMenuItems { show_hide });
    Ok(())
}

pub fn set_unread(app: &AppHandle, count: u32) {
    let label = show_hide_label(count);
    if let Some(items) = app.try_state::<TrayMenuItems>() {
        let _ = items.show_hide.set_text(&label);
    }
    // Tooltip is a NO-OP in tray-icon's Linux backend (kept for future platforms).
    if let Some(tray) = app.tray_by_id("main") {
        let tip = if count == 0 {
            "WhatsTauri".to_string()
        } else {
            format!("WhatsTauri — {count} unread")
        };
        let _ = tray.set_tooltip(Some(tip));
    }
}

fn toggle_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
}

/// Stock GNOME hides StatusNotifier trays without the AppIndicator extension (spec §6.4).
pub fn first_run_gnome_hint(app: &AppHandle) {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    if !desktop.to_uppercase().contains("GNOME") {
        return;
    }
    let Ok(dir) = app.path().app_config_dir() else {
        return;
    };
    let marker = dir.join(".gnome-hint-shown");
    if marker.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(&dir);
    // show() is a blocking D-Bus round-trip; off the main thread. Write the
    // marker only on a successful show so a failed notify is retried next launch.
    std::thread::spawn(move || {
        let shown = notify_rust::Notification::new()
            .summary("WhatsTauri tray icon on GNOME")
            .body(
                "Install the 'AppIndicator and KStatusNotifierItem Support' GNOME extension \
                   to see the tray icon. Without it, relaunch WhatsTauri to reopen the window.",
            )
            .appname("WhatsTauri")
            .show();
        if shown.is_ok() {
            let _ = std::fs::write(&marker, b"");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::show_hide_label;

    #[test]
    fn label_without_unread() {
        assert_eq!(show_hide_label(0), "Show/Hide WhatsApp");
    }

    #[test]
    fn label_with_unread() {
        assert_eq!(show_hide_label(3), "Show/Hide WhatsApp (3 unread)");
    }
}
