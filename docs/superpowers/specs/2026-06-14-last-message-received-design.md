# Last Message Received — Design

**Date:** 2026-06-14
**Status:** Approved (design), pending implementation
**Branch:** `icon-update`

## Problem

WhatsTauri sees every incoming message (the `window.Notification` shim forwards each to
`deliver_notification`), but once the native notification fades there is no way to glance at *what the
last message was* without opening WhatsApp. Goal: surface the most recent incoming message — **from /
date-time / message text** — in a place that survives the notification toast.

## Scope

- **In:** show the single most recent incoming message as read-only rows in the **tray menu**.
- **Out:** per-chat breakdown / history of more than one message, click-to-open behavior, persistence
  across app restart, a config toggle (see "Privacy"), taskbar surfacing.

## Why the tray menu

Tray **tooltips are a no-op** in tray-icon's GTK/Linux backend (`tray.rs:54`), so a hover popup is not an
option. The tray **menu** is the only functional Linux surface. The rows are **disabled** (greyed,
non-clickable) — purely informational.

## Privacy (decided: always on, no toggle)

Message previews are *already* shown without activating the app: native notifications display sender +
body on arrival, and on KDE they persist in the notification-history applet. The menu row adds only
**persistence** (it stays until the next message) — a small delta, not a new exposure class. A config
toggle is therefore over-engineering; it can be added later as a one-line `config.rs` key if wanted.

## Data source & timestamp

The only signal is the `Notification` payload (sender as `title`, message preview as `body`). It carries
**no timestamp**, so time is captured at notification time **in the browser** via `new Date()` — this
gives correct local time/locale with zero Rust date dependency.

- **Format ("smart"):** `"14:32"` if the message arrived today, else `"Jun 13 14:32"`.
- The `body` is WhatsApp's **preview** — usually full text, truncated for long messages, a placeholder
  for media ("📷 Photo"). `from` is shown **verbatim** as WhatsApp provides it (a group's `title` is the
  group name; the actual sender may be embedded in `body`).

## Architecture

Three small pieces, each mirroring an existing pattern.

### 1. Web shim — `inject/notification-shim.js`

Compute the smart timestamp and pass it as a third argument. Today:

```js
send("deliver_notification", { title: ..., body: ... });
```

becomes `{ title, body, ts }`, where `ts` is built at notification time:

```js
function fmtTs(d) {
  var now = new Date();
  var sameDay =
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate();
  var time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }); // "14:32"
  if (sameDay) return time;
  var date = d.toLocaleDateString([], { month: "short", day: "numeric" }); // "Jun 13"
  return date + " " + time;
}
```

Adding an argument does **not** change the command name, so `build.rs`, `capabilities/remote.json`, and
`gen/` are untouched.

### 2. New module — `src/last_message.rs` (pure, no stored state)

No message copy is retained in memory; labels are formatted and written straight to the menu.

```rust
/// "Alice · 14:32" — sender lightly truncated if very long.
pub fn from_label(from: &str, ts: &str) -> String;

/// Quoted, char-truncated preview: "see you at 5pm" → with "…" when clipped.
pub fn body_label(message: &str) -> String;
```

- Truncation is **char-aware** (multibyte-safe), like the existing `prepare_body` (`notifications.rs`).
- `from` truncation cap: 30 chars. `message` truncation cap: 50 chars. Ellipsis: `…` (U+2026).
- Separator/quote glyphs: from/ts joined by `" · "`; body wrapped in `“ ”` (U+201C / U+201D).

### 3. Tray — `src/tray.rs`

- `TrayMenuItems` gains two `MenuItem<Wry>` handles: `last_from`, `last_body`.
- `create()` builds the menu top-to-bottom as: `last_from`, `last_body`, separator,
  `show_hide`, `reload`, `clear`, `quit`. The two rows are created **disabled**.
- **Before the first message:** the rows + their separator are hidden. **Verify `MenuItem::set_visible`
  exists in tauri 2.11 during implementation**; if it does, create the rows hidden and show them on the
  first message. If it does **not**, fall back to inserting the rows dynamically at index 0 on the first
  message (guarded by a "already inserted" flag). The implementation plan must resolve this with a check,
  not an assumption.
- New `pub fn set_last_message(app: &AppHandle, from: &str, ts: &str, message: &str)`:
  formats via `last_message::from_label` / `body_label`, then on the main thread
  (`app.run_on_main_thread`) calls `set_text` on the two rows (and reveals them on first use).

### 4. `notifications.rs` & `lib.rs`

- `deliver_notification` gains `ts: String`; after firing the native notification it calls
  `crate::tray::set_last_message(&app, &title, &ts, &body)`.
- `lib.rs` registers `mod last_message;`. `invoke_handler!` is unchanged (same command name). No new
  managed state.

## Data flow

```
WhatsApp notification → shim FakeNotification → deliver_notification{title, body, ts}
   ├─ native notification (unchanged)
   └─ set_last_message → from_label / body_label → run_on_main_thread → set_text on the two rows
```

## Error handling

- Menu/row update and `run_on_main_thread` failures are ignored (`let _ =`) — surfacing the last message
  is cosmetic and must never break notification delivery.
- Native-notification behavior is completely unchanged.
- The shim and the binary ship together (embedded init script), so there is no `ts`-missing version skew;
  `ts` is a required `String`.

## Testing

Pure unit tests in `last_message.rs` (no GTK/display), following `unread.rs` / `badge.rs`:

- `from_label("Alice", "14:32") == "Alice · 14:32"`.
- `from_label` with a >30-char sender is truncated with `…` and still contains the ts.
- `body_label("see you")` is quoted and unchanged in substance; a >50-char message is clipped to the cap
  + `…`; a multibyte message (e.g. emoji) does not panic and stays valid UTF-8.
- `body_label` on a media placeholder ("📷 Photo") round-trips unchanged (within cap).

Existing `tray.rs` / `notifications.rs` / `unread.rs` / `badge.rs` tests stay green.

## Release / version bump

The `icon-update` branch ships two user-facing features (tray unread badge + last message received), so
merging it should cut a release. Per CLAUDE.md, `tag-on-version.yml` tags `v<version>` on push to `main`
only when the version changed, and CI's version-consistency job requires `tauri.conf.json` and
`Cargo.toml` to match.

- Bump **both** `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json` from `0.1.0` → **`0.2.0`** (minor:
  new backward-compatible features). This single bump covers everything on the branch.
- Done as the final task in the plan, after the feature is implemented and green, so the version reflects
  the merged result.

## CI / housekeeping

- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` must pass.
- **No** IPC command added/renamed → no `build.rs` / `invoke_handler!` name changes,
  no `capabilities/remote.json` change, no `gen/` regeneration.
- No new crate dependency (timestamp formatted in JS; truncation is std-only).
- Version bumped to `0.2.0` in both files (see "Release / version bump").

## Risks

- `MenuItem::set_visible` availability is unverified — the plan resolves it with a check + a defined
  fallback (dynamic insert), so it is not a blocker.
- Menu labels are static between updates — acceptable by design; the smart timestamp keeps an old row
  unambiguous.
