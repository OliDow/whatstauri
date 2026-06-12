use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::webview::{DownloadEvent, NewWindowResponse, WebviewWindowBuilder};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow};

const WHATSAPP_URL: &str = "https://web.whatsapp.com";

/// Subframe navigations and redirect hops can re-fire the handlers; don't spam
/// the user's browser. At most one external open per URL per 2 seconds.
fn open_external(url: &tauri::Url) {
    static LAST: Mutex<Option<(String, Instant)>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    if let Some((u, t)) = &*last {
        if *u == url.as_str() && now.duration_since(*t) < Duration::from_secs(2) {
            return;
        }
    }
    *last = Some((url.as_str().to_string(), now));
    let _ = tauri_plugin_opener::open_url(url.as_str(), None::<String>);
}

/// Last-requested download destination. wry shares one failed-flag across all
/// downloads per session, so `success` goes permanently false after one failed
/// download; we trust the filesystem over the flag. Single slot is enough —
/// downloads are user-initiated and serial in practice.
static DOWNLOAD_DESTINATION: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

/// Show the browser-wall notification at most once per session (the eprintln
/// still fires every time).
static WALL_NOTIFIED: AtomicBool = AtomicBool::new(false);

pub fn build_main_window(app: &AppHandle, cfg: &crate::config::Config) -> tauri::Result<()> {
    let url: url::Url = WHATSAPP_URL.parse().expect("static url");

    // Isolate webview storage under our identifier (and make wipe.rs effective);
    // WebKitGTK's default would be the SHARED ~/.local/share/webkitgtk dir.
    // This path must never change once released — sessions live here (spec §4).
    let data_dir = app.path().app_local_data_dir()?;

    // Belt-and-braces: WebKitGTK mkdirs on demand, but don't rely on it right
    // after a wipe removed the directory.
    std::fs::create_dir_all(&data_dir).ok();

    let win = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
        .title("WhatsTauri")
        .inner_size(1100.0, 750.0)
        .visible(!cfg.start_hidden)
        .user_agent(&cfg.user_agent)
        .data_directory(data_dir)
        .disable_drag_drop_handler() // let HTML5 file drag-and-drop reach WhatsApp (spec §3.1)
        .initialization_script(include_str!("../inject/notification-shim.js"))
        .initialization_script(include_str!("../inject/title-watcher.js"))
        .on_navigation(move |url| {
            if crate::links::is_browser_wall(url) {
                eprintln!(
                    "whatstauri: WhatsApp served the unsupported-browser page. \
                     Remedies: bump user_agent in config.toml, or tray > Clear data & re-login."
                );
                if !WALL_NOTIFIED.swap(true, Ordering::Relaxed) {
                    crate::notifications::notify(
                        "WhatsTauri: browser check failed",
                        "WhatsApp rejected the current user agent. Edit user_agent in \
                           config.toml or use 'Clear data & re-login' from the tray.",
                    );
                }
                return true; // still show the page; it names what's missing
            }
            if crate::links::is_internal(url) {
                return true;
            }
            open_external(url);
            false
        })
        .on_new_window(move |url, _features| {
            // wry/WebKitGTK silently swallows target="_blank" / window.open without
            // this handler. Every popup is denied (no unmanaged windows), so the
            // system browser is the only place a new-window target can live —
            // including internal ones like faq.whatsapp.com / whatsapp.com/legal.
            open_external(&url);
            NewWindowResponse::Deny
        })
        .on_download(move |_webview, event| {
            match event {
                DownloadEvent::Requested { destination, .. } => {
                    // WebKitGTK destination override is historically flaky (spec §3.1);
                    // if downloads stall, fall back to moving the file in Finished.
                    if let Some(dir) = dirs::download_dir() {
                        let name = destination
                            .file_name()
                            .map(|s| s.to_os_string())
                            .unwrap_or_else(|| "whatsapp-download".into());
                        *destination = dir.join(name);
                    }
                    // Remember where we asked the file to land; wry's shared
                    // failed-flag makes `success` unreliable (see static doc).
                    *DOWNLOAD_DESTINATION
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = Some(destination.clone());
                }
                DownloadEvent::Finished { path, success, .. } => {
                    // Trust the filesystem over wry's sticky `success` flag.
                    let effective = path.or_else(|| {
                        DOWNLOAD_DESTINATION
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .take()
                    });
                    let body = match effective {
                        Some(p) if p.exists() => format!("Saved {}", p.display()),
                        _ if success => "Download finished".to_string(),
                        _ => "Download failed".to_string(),
                    };
                    crate::notifications::notify("WhatsTauri download", &body);
                }
                _ => {}
            }
            true
        })
        .build()?;

    install_crash_recovery(&win);
    Ok(())
}

/// Native reload — respawns the web process if it has terminated (eval-based
/// reload cannot run JS in a dead process).
pub(crate) fn reload_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        #[cfg(target_os = "linux")]
        {
            let r = w.with_webview(|pw| {
                use webkit2gtk::WebViewExt;
                pw.inner().reload();
            });
            if let Err(e) = r {
                eprintln!("whatstauri: native reload failed: {e}");
                let _ = w.eval("location.reload()");
            }
        }
        #[cfg(not(target_os = "linux"))]
        let _ = w.eval("location.reload()");
    }
}

/// Auto-reload on WebKitGTK web-process crash, max 3 per minute (spec §5.2).
/// Tauri doesn't expose the signal; the raw webkit2gtk handle does.
fn install_crash_recovery(win: &WebviewWindow) {
    #[cfg(target_os = "linux")]
    {
        use std::cell::RefCell;
        use std::rc::Rc;

        let install = win.with_webview(|platform_webview| {
            use webkit2gtk::WebViewExt;
            let wv = platform_webview.inner();
            let crashes: Rc<RefCell<Vec<Instant>>> = Rc::new(RefCell::new(Vec::new()));
            wv.connect_web_process_terminated(move |wv, reason| {
                eprintln!("whatstauri: web process terminated ({reason:?})");
                let now = Instant::now();
                let mut c = crashes.borrow_mut();
                c.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
                c.push(now);
                if c.len() <= 3 {
                    wv.reload();
                } else {
                    crate::notifications::notify(
                        "WhatsTauri keeps crashing",
                        "The web view crashed repeatedly. Try setting \
                           disable_dmabuf_renderer = true in config.toml.",
                    );
                }
            });
        });
        if let Err(e) = install {
            eprintln!("whatstauri: failed to install crash recovery: {e}");
        }
    }
}
