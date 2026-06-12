use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};

/// In-flight click-waiter threads; beyond the cap we fire-and-forget (GNOME
/// keeps actioned notifications alive until dismissed, so waiters accumulate).
static WAITERS: AtomicUsize = AtomicUsize::new(0);
const MAX_WAITERS: usize = 16;

/// Last unread count — report_title keeps previous value on unparseable titles.
pub struct UnreadState(pub Mutex<u32>);

fn escape_body(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Whether the notification daemon advertises body-markup (escape only then —
/// plain-text daemons would render literal entities). Queried once, lazily,
/// off the main thread.
fn body_markup_supported() -> bool {
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(|| {
        notify_rust::get_capabilities()
            .map(|caps| caps.iter().any(|c| c == "body-markup"))
            .unwrap_or(true) // unknown daemon: escape defensively
    })
}

fn prepare_body(body: &str) -> String {
    let truncated: String = body.chars().take(1000).collect();
    if body_markup_supported() {
        escape_body(&truncated)
    } else {
        truncated
    }
}

/// Fire-and-forget native notification. Never blocks the caller:
/// notify-rust's show() is a synchronous D-Bus round-trip, which must not run
/// on the GTK main thread (webview callbacks, menu handlers).
pub(crate) fn notify(summary: &str, body: &str) {
    let summary = summary.to_string();
    let body = body.to_string();
    std::thread::spawn(move || {
        if let Err(e) = notify_rust::Notification::new()
            .summary(&summary)
            .body(&prepare_body(&body))
            .appname("WhatsTauri")
            .show()
        {
            eprintln!("whatstauri: notification failed: {e}");
        }
    });
}

/// Web Notification → native notification with click-to-focus (spec §3.2).
/// notify-rust's D-Bus "default" action fires when the notification body is clicked.
#[tauri::command]
pub fn deliver_notification(app: AppHandle, title: String, body: String) {
    std::thread::spawn(move || {
        let shown = notify_rust::Notification::new()
            .summary(&title)
            .body(&prepare_body(&body))
            .appname("WhatsTauri")
            .action("default", "Open")
            .show();

        let slot = WAITERS.fetch_add(1, Ordering::AcqRel);
        if slot >= MAX_WAITERS {
            WAITERS.fetch_sub(1, Ordering::AcqRel);
            eprintln!(
                "whatstauri: {MAX_WAITERS} notifications pending — click-to-focus disabled for this one"
            );
            if let Err(e) = shown {
                eprintln!("whatstauri: notification failed: {e}");
            }
            return;
        }

        match shown {
            Ok(handle) => handle.wait_for_action(|action| {
                if action == "default" {
                    focus_main(&app);
                }
            }),
            Err(e) => eprintln!("whatstauri: notification failed: {e}"),
        }
        WAITERS.fetch_sub(1, Ordering::AcqRel);
    });
}

#[tauri::command]
pub fn report_title(app: AppHandle, title: String) {
    let state = app.state::<UnreadState>();
    let changed = {
        let mut last = state.0.lock().unwrap_or_else(|e| e.into_inner());
        let n = crate::unread::parse_unread(&title, *last);
        if n != *last {
            *last = n;
            Some(n)
        } else {
            None
        }
    };
    if let Some(n) = changed {
        crate::tray::set_unread(&app, n);
    }
}

/// Init-script lifecycle logging, active only with --debug (spec §5.5).
#[tauri::command]
pub fn debug_log(message: String) {
    static DEBUG: OnceLock<bool> = OnceLock::new();
    if *DEBUG.get_or_init(|| std::env::args().any(|a| a == "--debug")) {
        eprintln!("[whatstauri:init] {}", message.escape_debug());
    }
}

pub fn focus_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}
