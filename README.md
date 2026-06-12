# WhatsTauri

[![CI](https://github.com/OliDow/whatstauri/actions/workflows/ci.yml/badge.svg)](https://github.com/OliDow/whatstauri/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/OliDow/whatstauri?label=release)](https://github.com/OliDow/whatstauri/releases)
[![Tauri 2](https://img.shields.io/badge/Tauri-2.x-24C8DB?logo=tauri&logoColor=white)](https://v2.tauri.app)
[![Platform](https://img.shields.io/badge/platform-linux-FCC624?logo=linux&logoColor=black)](https://github.com/OliDow/whatstauri/releases)
[![License](https://img.shields.io/github/license/OliDow/whatstauri)](LICENSE)

A lightweight, unofficial Linux desktop wrapper for [WhatsApp Web](https://web.whatsapp.com),
built with Tauri 2 (Rust + system WebKitGTK). Created as a replacement for the archived
whatsapp-for-linux/WasIstLos.

Unofficial: not affiliated with or endorsed by Meta/WhatsApp. It wraps the genuine
WhatsApp Web client and performs no automation.

## Features

- WhatsApp Web with persistent login (isolated webview storage)
- System tray with unread count, close-to-tray, background running
- Native notifications with click-to-focus
- Media downloads to ~/Downloads, external links open in your browser
- Auto-recovery from web-process crashes (max 3 reloads/minute)
- NVIDIA auto-quirks: full GPU rendering on NVIDIA + Wayland out of the box
  (works around WebKit bug 280210 and flaky in-sandbox NVDEC)

## Known limitations (engine, not app)

- No voice/video calls and no voice-message recording: WebKitGTK ships without
  WebRTC/getUserMedia. The realistic future unlock is Tauri/wry migrating to WebKitGTK 6.0.
- GNOME tray: install the "AppIndicator and KStatusNotifierItem Support" extension
  (KDE, XFCE, Cinnamon work out of the box). The app hints this on first run under GNOME.
- NVIDIA + X11 (GTK3) cannot render accelerated — WebKit requires linear GBM buffers there,
  which NVIDIA hardware can't render to. Use a Wayland session.
- Video playback needs an H.264 decoder: stock Fedora works once `gstreamer1-plugin-openh264` is installed (default-enabled Cisco repo; the RPM recommends it), or use RPM Fusion codecs (`libavcodec-freeworld`) for maximum robustness.

## Config

`~/.config/whatstauri/config.toml` (all keys optional):

```toml
user_agent = "Mozilla/5.0 ..."   # bump when WhatsApp raises its browser floor
close_to_tray = true
start_hidden = false
nvidia_quirks = true             # auto-apply NVIDIA workarounds when an NVIDIA GPU is detected
force_shm = false                # compatibility: GPU paint, shared-memory transport
disable_dmabuf_renderer = false  # last resort: full software rendering
disable_compositing = false      # last resort: disables accelerated compositing (breaks video)
```

Rendering escalation path if you see glitches: defaults (GPU) → `force_shm = true` →
`disable_dmabuf_renderer = true`.

App-internal state markers live in `~/.config/com.shatteredsun.whatstauri/` (separate from this config file, which "Clear data & re-login" deliberately preserves).

## Build

```bash
sudo dnf install webkit2gtk4.1-devel openssl-devel libappindicator-gtk3-devel \
  librsvg2-devel rpm-build rust cargo gcc gcc-c++ make curl wget file
cargo install tauri-cli --locked
cd src-tauri && cargo tauri build    # RPM/deb/AppImage in target/release/bundle/
```

## Troubleshooting

- "Browser not supported" page: raise the Chrome version in `user_agent`, restart; if it
  persists (service worker cached the verdict), use tray > "Clear data & re-login".
- Window crashes or renders black on NVIDIA: confirmed working path is Wayland with the
  default auto-quirks. If artifacts appear, try `force_shm = true`.
- Repeated web-process crashes: the app auto-reloads up to 3×/min, then notifies; set
  `disable_dmabuf_renderer = true` as a fallback.
- Debug init-script and bridge activity: run with `--debug`.
- AppImage builds are best-effort and have known GStreamer fragility; prefer the RPM.
