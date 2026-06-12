# Release smoke checklist

Run against a release build (`cargo tauri build`, install the RPM) before tagging.

1. [ ] App boots to WhatsApp (QR or logged-in chat list) — NOT the unsupported-browser page
2. [ ] stderr with `--debug`: notification-shim installed, IPC ready, title-watcher attached
3. [ ] On NVIDIA + Wayland: no `Error 71` crash, no GBM buffer errors (auto-quirks engaged —
       verify with `tr '\0' '\n' < /proc/$(pgrep -f WebKitWebProcess | head -1)/environ | grep __NV`)
4. [ ] QR login works (fresh profile)
5. [ ] Session survives app restart (no QR re-scan)
6. [ ] Message received while window hidden → native notification → body click focuses window
7. [ ] Unread count appears in tray tooltip; clears when read
8. [ ] Photo download lands in ~/Downloads (blob URL) + completion notification
9. [ ] External link in a chat opens in default browser (target=_blank path)
10. [ ] Video message plays (H.264 via openh264 — NVDEC demoted on NVIDIA)
11. [ ] Voice note plays (Opus)
12. [ ] ✕ hides to tray; tray Show/Hide restores; Quit exits fully
13. [ ] Second launch while running focuses the existing window
14. [ ] Clear data & re-login: confirms, restarts, shows QR page
