# WhatsTauri — Design Spec

**Date:** 2026-06-11
**Status:** Draft — verification complete (§9), pending user review
**Target:** Linux only (Fedora primary), RPM-first packaging

## 1. Overview

A minimal, pure-Rust Tauri 2 desktop wrapper around `https://web.whatsapp.com` — a lightweight replacement for the now-archived `whatsapp-for-linux`/WasIstLos (WebKitGTK) app, with system tray, background running, and native notifications.

**Why this exists:** the app currently in use (`whatsapp-for-linux` 1.6.5) is dead upstream (archived March 2026). Chromium-based alternatives (ZapZap, Electron wrappers) cost 1–2.5 GB RAM. A Tauri/WebKitGTK wrapper lands in the ~200–400 MB class while delivering the same daily-driver feature set.

**Validation:** wrapping the genuine web.whatsapp.com client is now Meta's own official approach on Windows (WebView2 wrapper since Nov 2025). No major wrapper project shows any pattern of account bans from passive embedding — bans target automation, which this app does none of.

### Goals (v1)

1. WhatsApp Web in a window, with persistent login across restarts
2. System tray: unread count signal, show/hide, close-to-tray, background running
3. Native desktop notifications bridged from the web app
4. Media downloads (blob URLs) saved to `~/Downloads`
5. External links open in the system browser
6. Single instance — relaunching focuses the existing window

### Non-goals (v1)

- **Voice/video calls and voice-message recording** — engine-gated: webkit2gtk-4.1 ships without WebRTC/getUserMedia on all mainstream distros. Accepted limitation (same as the current app). See §11.
- Multi-account, autostart, window-state persistence, spell check, notification avatars, themes — all deferred; each is a small additive change later.
- Any form of message automation (ToS/ban-risk boundary — this app never injects into WhatsApp's internals).

## 2. Architecture

Pure-Rust Tauri 2 app (latest stable 2.x — 2.11.2 at design time; declared as `tauri = "2"` so updates flow with `cargo update`). No Node, no bundled frontend, no build step beyond `cargo tauri build`. The "frontend" is the remote URL itself; native behavior lives in Rust plus small JavaScript init-scripts injected before WhatsApp's code runs.

```
whatstauri/
└── src-tauri/
    ├── tauri.conf.json        # window, bundle (.rpm/.deb/AppImage), identifier
    ├── capabilities/
    │   └── remote.json        # ONLY grant: notification/badge bridge commands, to web.whatsapp.com
    ├── inject/
    │   ├── notification-shim.js
    │   └── title-watcher.js
    └── src/
        ├── main.rs / lib.rs   # builder wiring, plugins
        ├── window.rs          # webview creation: UA, init scripts, downloads, link policy
        ├── tray.rs            # tray icon, menu, unread-count state
        ├── notifications.rs   # IPC commands: web notification → native; unread count → tray
        └── config.rs          # settings file (TOML in XDG config dir)
```

**Security posture:** the remote page receives no Tauri access except the explicitly whitelisted bridge commands, granted via a capability file with `"remote": {"urls": ["https://web.whatsapp.com"]}`. Downloads, navigation policy, and tray are handled engine-side in Rust where page JS cannot reach. All WhatsApp traffic (WebSocket, media, E2E crypto) flows directly between the page and Meta's servers — nothing is proxied, message content is never read. Keep Tauri current (history of remote-origin IPC advisories, e.g. CVE-2024-35222).

**Engine-agnostic discipline:** all WhatsApp-facing logic (UA config, shims, Rust handlers) avoids WebKitGTK-specific assumptions so a future wry migration to WebKitGTK 6.0 — the realistic path to calls — is an enablement, not a rewrite.

## 3. Components

### 3.1 Window (`window.rs`)

- Main webview at `https://web.whatsapp.com`, user-agent from config (default: a current Chrome-on-Linux string, see §6.3 UA rot).
- Injects both init scripts via `initialization_script`.
- `on_download`: route downloads to `~/Downloads` (XDG download dir), raise a completion notification. Verified: blob-URL downloads fire the handler on WebKitGTK (the hard breakage is macOS-only). Destination-path override is historically flaky on WebKitGTK — if it misbehaves, accept the engine's default path and move the file on `DownloadEvent::Finished`. JS blob→IPC fallback kept documented but not expected to be needed.
- `on_navigation` + `on_new_window`: allowlist `*.whatsapp.com` / `*.whatsapp.net` in-app; everything else opens via `tauri-plugin-opener` in the system browser.
- Close button hides to tray instead of quitting (configurable; auto-disabled if no tray is available — §6.4).
- `dragDropEnabled: false` so HTML5 drag-and-drop file uploads reach WhatsApp instead of the Tauri handler.

### 3.2 Notification bridge (`notification-shim.js` + `notifications.rs`)

- Shim replaces `window.Notification`: `permission` always `'granted'`, constructor forwards `{title, body}` over IPC (icon/avatar deliberately dropped in v1).
- IPC mechanism (verified): init scripts get `window.__TAURI_INTERNALS__.invoke` on remote pages without `withGlobalTauri`; the shim retries until it exists (known init-ordering race, tauri #12404). `withGlobalTauri` stays **off** — the page never sees the full API surface.
- Rust raises native notifications via **`notify-rust` directly, not `tauri-plugin-notification`** — the plugin has no click/action support on desktop (actions are mobile-only; open feature request plugins-workspace #2150) and has reported silent failures on GNOME 46+. `notify-rust`'s D-Bus `"default"` action fires on notification-body click → un-hide + focus window (spawned on the async runtime; works on GNOME/KDE, degrades to no-click on minimal notification daemons).
- Bypasses WebKitGTK's own notification plumbing entirely.

### 3.3 Tray + unread count (`tray.rs` + `title-watcher.js`)

- Title watcher observes `document.title` (WhatsApp maintains `"(n) WhatsApp"` itself — the most churn-resistant unread signal; DOM selectors explicitly avoided).
- Parse rule: regex `^\((\d+)\)`; on parse failure, keep the previous count (a flaky title must never wipe a real count).
- Tray shows count via tooltip/icon state. **Menu-driven only** (verified: with the appindicator backend, tray click events are not emitted on Linux — tray-icon #104). Menu, in order: **Show/Hide WhatsApp** (first item, since left-click-to-toggle is impossible), Reload, Clear data & re-login, Quit.
- GNOME caveat (verified): Tauri's tray uses libayatana-appindicator/StatusNotifier; stock GNOME shows nothing without the "AppIndicator and KStatusNotifierItem Support" shell extension. First-run hint when GNOME is detected without StatusNotifier support. `single-instance` focus-on-relaunch is the designed universal "show window" recovery path.

### 3.4 Config (`config.rs`)

TOML in XDG config dir. v1 keys: `user_agent`, `close_to_tray`, `start_hidden`, plus passthrough env workarounds (§5.2). Malformed config → defaults + logged warning, never a refusal to start. No settings UI in v1.

### 3.5 Plugins & key crates

Plugins: `single-instance`, `opener`. Crates: `notify-rust` (notifications with click actions — replaces the notification plugin, see §3.2), `webkit2gtk` (crash-signal hook, see §5.2; version pinned to match Tauri's own dependency). Nothing else in v1.

## 4. Data flow

Only three things cross the page↔Rust boundary:

1. **Notifications:** shimmed `Notification` → IPC `deliver_notification(title, body)` → native notification (notify-rust) → body-click fires the D-Bus `"default"` action → un-hide + focus window.
2. **Unread count:** title watcher → IPC event with parsed count → tray state.
3. **Downloads:** engine-level `on_download` (not page JS) → file in `~/Downloads` → completion notification.

Session state (login keys, messages) lives entirely in the webview's own storage (IndexedDB/localStorage — WhatsApp sessions are client-side, not server cookies) under `~/.local/share/<identifier>/`. We never read it; we only ensure it persists:

- **Never** set `incognito`.
- **Never** change the bundle identifier between releases (it moves the data dir → mass logout).
- Treat the UA string as part of session state: changing it mid-session can resurface the cached "unsupported browser" verdict via the service worker; remedy is §5.1.

## 5. Error handling & resilience

### 5.1 "Browser not supported" page

The canonical failure mode (UA version floor raised, or service worker cached a stale verdict). Detection: navigation to `browsers.html` → log a visible warning suggesting the two remedies: edit `user_agent` in config, or tray → "Clear data & re-login".

### 5.2 Web process crash — auto-reload (in scope)

WebKitGTK renderer crashes (the GPU/compositing instability class). Tauri doesn't expose the crash signal, but `with_webview` hands us the raw `webkit2gtk::WebView`, where `connect_web_process_terminated` → `reload()` gives auto-recovery (runs on the GTK main thread). Guarded by a crash-loop limit (max 3 reloads per minute, then stop and raise a native notification telling the user). Tray "Reload" remains as manual recovery. README + config document the `WEBKIT_DISABLE_DMABUF_RENDERER=1` / `WEBKIT_DISABLE_COMPOSITING_MODE=1` workarounds for NVIDIA/Wayland glitches.

### 5.3 Clear data & re-login

Race-free wipe: confirm dialog → write "wipe on next start" marker → restart app → delete webview data dir before webview init. (Never delete the dir while the engine holds it open. Note WhatsApp's own logout does not wipe client storage — this action is also the privacy-complete account removal.)

### 5.4 Offline

WhatsApp Web renders its own offline banner; not duplicated.

### 5.5 Diagnostics

`--debug` flag logs init-script lifecycle events (shim installed, title watcher attached, IPC delivery) to stderr, so "it broke after a WhatsApp update" is diagnosable to a specific seam in minutes.

## 6. Known risks & mitigations

1. **Engine compatibility wall (existential):** WhatsApp Web treats WebKitGTK as unsupported; this killed WasIstLos. Mitigations: WebKitGTK ≥ 2.46.1 required (QR-render fix; Fedora 44 satisfies this), runtime-editable UA, clear-data escape hatch, minimal injected surface (title + Notification shim only — no DOM selectors), `browsers.html` detection. Accepted residual risk: a future WhatsApp change could break WebKit support in a way no wrapper code can fix.
2. **GPU/rendering on NVIDIA (resolved during v1 bring-up):** WebKitGTK's GPU path crashes on NVIDIA+Wayland (WebKit bug 280210, explicit-sync) and is structurally broken on NVIDIA+X11 (linear GBM buffers). The app auto-applies quirks when an NVIDIA GPU is detected (`nvidia_quirks = true` default): `__NV_DISABLE_EXPLICIT_SYNC=1` on Wayland (keeps the full GPU path — verified dramatically smoother than software fallbacks) and `GST_PLUGIN_FEATURE_RANK` demotion of flaky NVDEC decoders (restores video playback via openh264). Escalating fallbacks remain config-selectable: `force_shm` (GPU paint, SHM transport), then `disable_dmabuf_renderer`/`disable_compositing` (software rendering). See §5.2 for crash auto-reload.
3. **UA rot:** default UA carries a Chrome version that goes stale. Default is bumped each release; config override bridges the gap between releases; `browsers.html` detection makes the failure self-diagnosing.
4. **No tray on stock GNOME:** the app shows a one-time hint (install the AppIndicator extension) when GNOME is detected, and `single-instance` focus-on-relaunch is the universal "show window" recovery. `close_to_tray` remains user-configurable rather than auto-disabled (tray creation succeeds even when GNOME hides it, so reliable detection would require a D-Bus StatusNotifierWatcher probe — deferred). README documents the extension.
5. **Hidden-window throttling (verified, acceptable):** WebKitGTK throttles DOM timers (~1 s granularity) for hidden views but does **not** suspend the web process — WebSocket message delivery continues, so background notifications work. Insurance against timer-driven sluggishness: the title-watcher init script holds a `navigator.locks` WebLock (the workaround Tauri's own docs suggest); visibility-state spoofing is held in reserve if WhatsApp's own "inactive" mode proves problematic. No confirmed reports exist of Linux tray apps missing WebSocket messages.
6. **Codecs (verified):** voice-note playback (Opus) works on stock Fedora out of the box (`gst-plugins-base` ships the decoder). Video playback (H.264 Main/High) needs extra codecs: RPM Fusion (reliable) or the default-enabled Cisco openh264 repo (`gstreamer1-plugin-openh264`; baseline-profile caveat on older versions). RPM declares GStreamer `Recommends:`; README documents both routes.

## 7. Testing

Live third-party site — cannot pin versions (the service worker updates underneath us). Layered approach:

- **Rust unit tests** for pure logic: title parsing (incl. failure-keeps-previous rule), link-policy allowlist, config load/fallback.
- **Manual release smoke checklist** (in repo): WhatsApp boots with shims active (first item — the realistic breakage point); QR login; session survives restart; receive message → native notification (→ click focuses, if supported); unread count in tray; download a photo (blob URL); external link opens in browser; close-to-tray + background message arrives while hidden; second launch focuses.

## 8. Packaging & distribution

- `cargo tauri build` → **.rpm** (primary), .deb and AppImage as best-effort extras (AppImage has known GStreamer fragility for media — documented, not blocking).
- RPM dependencies: `webkit2gtk4.1 >= 2.46.1`, GStreamer base/good as `Recommends:`, libappindicator noted for tray.
- Identifier fixed at first release and never changed (§4).
- No auto-updater in v1 (personal-use RPM); revisit if distributed.
- Unofficial-client note: README carries the standard "unofficial, not affiliated with Meta; wraps the genuine WhatsApp Web client; no automation" disclaimer.

## 9. Verification results (resolved 2026-06-11)

Engine-behavior assumptions were researched against upstream issues/docs before finalizing this spec; resolutions are folded into §§3, 5, 6, 8 above. Summary:

| # | Assumption | Verdict | Design consequence |
|---|---|---|---|
| 1 | Notification click via tauri-plugin-notification | **Broken** — desktop actions unsupported (plugins-workspace #2150); GNOME 46+ silent failures reported | Use `notify-rust` directly; D-Bus `"default"` action = body click → focus (§3.2) |
| 2 | Blob downloads fire `on_download` on WebKitGTK | **Verified** — Linux is the good platform (hard breakage is macOS-only) | Destination override flaky → move-on-Finished fallback; JS shim unneeded (§3.1) |
| 3 | Hidden window keeps receiving messages | **Verified-acceptable** — timers throttled ~1 s, WebSockets unaffected, process not suspended | Close-to-tray stands; WebLock insurance in init script (§6.5) |
| 4 | Tray works on Fedora GNOME | **Confirmed problem** — needs AppIndicator extension; additionally **no click events on Linux** (tray-icon #104) | Menu-driven tray, Show/Hide as first item; first-run hint (§3.3) |
| 5 | Crash signal exposed by Tauri/wry | **Not exposed** — but reachable | `with_webview` → `webkit2gtk::WebView::connect_web_process_terminated` → guarded auto-reload (§5.2) |
| 6 | Remote-URL IPC recipe | **Verified** — capability `remote.urls` gates ACL; `__TAURI_INTERNALS__.invoke` exists on remote pages without `withGlobalTauri` | Keep `withGlobalTauri` off; shim retries until invoke exists (§3.2) |
| 7 | Opus OOTB / H.264 needs codecs on Fedora | **Verified** | RPM `Recommends:`; README documents RPM Fusion + openh264 routes (§6.6) |

## 10. Success criteria

- Daily-drivable replacement for whatsapp-for-linux: login persists, chats work, notifications arrive (including while hidden), downloads save, tray behaves.
- Idle RSS materially below ZapZap/Electron class on the same account (target: under half).
- Surviving a WhatsApp Web release without code changes in the common case (UA bump at most).

## 11. Future work (explicitly out of v1)

- **Voice/video calls + voice recording:** blocked on the engine. Realistic unlock: wry/Tauri migration to WebKitGTK 6.0 (GTK4) — Karere proved calls work there with WebRTC enabled. Our engine-agnostic layering (§2) keeps this an enablement, not a rewrite. Distros enabling WebRTC in 4.1 is possible but historically hasn't happened.
- Autostart (`tauri-plugin-autostart`) and window-state (`tauri-plugin-window-state`) — one-liners when wanted.
- Notification avatars (blob fetch over IPC), spell check, deep links (`whatsapp://`), multi-account, theme polish.
