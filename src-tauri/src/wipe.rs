use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

fn marker_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join(".wipe-on-start"))
}

/// Tray menu entry: confirm (non-blocking — menu events run on the GTK main
/// thread, where a blocking dialog would deadlock), then mark + restart.
pub fn request_wipe(app: &AppHandle) {
    let app2 = app.clone();
    app.dialog()
        .message(
            "This deletes all local WhatsApp data and logs you out.\n\
             You will need to scan the QR code again.",
        )
        .title("Clear data & re-login")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancel)
        .show(move |confirmed| {
            if !confirmed {
                return;
            }
            let Some(marker) = marker_path(&app2) else {
                eprintln!("whatstauri: cannot resolve config dir; wipe aborted");
                crate::notifications::notify(
                    "WhatsTauri",
                    "Could not start the data wipe — see terminal output.",
                );
                return;
            };
            if let Some(parent) = marker.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::write(&marker, b"") {
                Ok(()) => app2.restart(),
                Err(e) => {
                    eprintln!("whatstauri: could not write wipe marker: {e}");
                    crate::notifications::notify(
                        "WhatsTauri",
                        "Could not start the data wipe — see terminal output.",
                    );
                }
            }
        });
}

/// Runs in setup() BEFORE the webview opens its storage.
pub fn handle_pending_wipe(app: &AppHandle) {
    let Some(marker) = marker_path(app) else {
        return;
    };
    if !marker.exists() {
        return;
    }
    if let Err(e) = std::fs::remove_file(&marker) {
        eprintln!("whatstauri: could not remove wipe marker: {e}");
    }
    let Ok(data_dir) = app.path().app_local_data_dir() else {
        return;
    };
    // app_local_data_dir must stay webview-only: wipe removes it wholesale.
    // The previous instance's WebKit network process may still be flushing
    // (restart is spawn-then-exit) — retry briefly before giving up.
    for attempt in 1..=3 {
        match std::fs::remove_dir_all(&data_dir) {
            Ok(()) => {
                eprintln!("whatstauri: webview data wiped ({})", data_dir.display());
                return;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return, // already gone
            Err(e) if attempt < 3 => {
                eprintln!("whatstauri: wipe attempt {attempt} failed ({e}), retrying…");
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(e) => {
                eprintln!("whatstauri: wipe failed: {e}");
                crate::notifications::notify(
                    "WhatsTauri",
                    "Clearing data failed — your WhatsApp session may still be present. \
                     Use 'Clear data & re-login' to retry.",
                );
            }
        }
    }
}
