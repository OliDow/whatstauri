# Last Message Received Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show the most recent incoming WhatsApp message (from · time, and the text) as read-only rows at the top of the tray menu.

**Architecture:** The existing `window.Notification` shim already forwards each message to `deliver_notification`; it gains a JS-computed local timestamp `ts`. A new pure `last_message.rs` formats the menu labels (truncation only — no stored state). `tray.rs` holds two disabled menu items and inserts them (plus a separator) at the top of the menu on the first message via `Menu::insert`, updating their text on the main thread thereafter.

**Tech Stack:** Rust, Tauri 2.11 menu API (`MenuItem::set_text`, `Menu::insert`, `PredefinedMenuItem::separator`), `AppHandle::run_on_main_thread`. No new crates (timestamp formatted in JS; truncation is std-only).

**Spec:** `docs/superpowers/specs/2026-06-14-last-message-received-design.md`

> **Note on commits:** This environment's git hook blocks `git commit`/`git add`. The per-task "commit" steps from the standard template are intentionally omitted — leave all changes **unstaged** for the user to review and commit. Run the verification steps as written.

---

## API facts (verified against tauri 2.11.2 source)

- `MenuItem::set_text` (`menu/normal.rs:102`) and `set_enabled` (`:113`) exist; **`set_visible` does not** → rows are added dynamically, not hidden/shown.
- `Menu::insert(&dyn IsMenuItem, position)` (`menu/menu.rs:328`) exists → used to add rows at the top on first message.
- `PredefinedMenuItem::separator(manager)` (`menu/predefined.rs:15`) exists.
- `AppHandle::run_on_main_thread` (`app.rs:1246`) — GTK menu mutation must run here.

---

## File Structure

- **Create** `src-tauri/src/last_message.rs` — pure label formatting (`from_label`, `body_label`, char-aware `truncate`). No Tauri types → unit-testable headless.
- **Modify** `src-tauri/src/lib.rs` — register `mod last_message;`.
- **Modify** `src-tauri/src/tray.rs` — extend `TrayMenuItems`; build the (initially-detached) rows in `create`; add `set_last_message`.
- **Modify** `src-tauri/src/notifications.rs` — `deliver_notification` gains `ts: String` and calls `set_last_message`.
- **Modify** `src-tauri/inject/notification-shim.js` — compute and send `ts`.
- **Modify** `src-tauri/Cargo.toml` + `src-tauri/tauri.conf.json` — version `0.1.0` → `0.2.0`.

All `cargo` commands run from `src-tauri/`.

---

## Task 1: `last_message.rs` — pure label formatting (TDD)

**Files:**
- Create: `src-tauri/src/last_message.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Register the module**

In `src-tauri/src/lib.rs`, add `mod last_message;` to the module list (keep it ordered):

```rust
mod badge;
mod config;
mod last_message;
mod links;
mod notifications;
mod tray;
mod unread;
mod window;
mod wipe;
```

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/src/last_message.rs`:

```rust
//! Formats the "last message received" rows shown in the tray menu.
//! Spec: docs/superpowers/specs/2026-06-14-last-message-received-design.md
//! Pure string formatting — no Tauri/GTK types, so it unit-tests headless.

const FROM_MAX: usize = 30;
const BODY_MAX: usize = 50;

/// Char-aware truncation: returns `s` unchanged if it fits in `max` chars,
/// otherwise the first `max` chars followed by an ellipsis. Multibyte-safe.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

/// Top row: "Alice · 14:32". Sender truncated if very long; `ts` is the
/// already-formatted local time string from the web shim.
pub fn from_label(from: &str, ts: &str) -> String {
    format!("{} · {}", truncate(from, FROM_MAX), ts)
}

/// Second row: the message preview, truncated and quoted: “see you at 5pm”.
pub fn body_label(message: &str) -> String {
    format!("“{}”", truncate(message, BODY_MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_label_basic() {
        assert_eq!(from_label("Alice", "14:32"), "Alice · 14:32");
    }

    #[test]
    fn from_label_truncates_long_sender() {
        let long = "A".repeat(40);
        let out = from_label(&long, "14:32");
        assert!(out.contains('…'), "long sender should be truncated: {out}");
        assert!(out.ends_with("· 14:32"), "ts must survive: {out}");
        // 30 kept chars + the ellipsis = 31 chars before " · 14:32".
        let name_part = out.split(" · ").next().unwrap();
        assert_eq!(name_part.chars().count(), 31);
    }

    #[test]
    fn body_label_quotes_and_keeps_short_text() {
        assert_eq!(body_label("see you at 5pm"), "“see you at 5pm”");
    }

    #[test]
    fn body_label_truncates_long_message() {
        let long = "x".repeat(80);
        let out = body_label(&long);
        assert!(out.contains('…'), "long body should be truncated: {out}");
        // strip the surrounding quotes, then 50 chars + ellipsis = 51.
        let inner = out.trim_start_matches('“').trim_end_matches('”');
        assert_eq!(inner.chars().count(), 51);
    }

    #[test]
    fn body_label_multibyte_does_not_panic() {
        // Media placeholder + emoji must stay valid UTF-8 and not panic.
        assert_eq!(body_label("📷 Photo"), "“📷 Photo”");
        let emojis = "😀".repeat(80);
        let out = body_label(&emojis);
        assert!(out.contains('…'));
        assert!(std::str::from_utf8(out.as_bytes()).is_ok());
    }
}
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test last_message::`
Expected: 5 passed. (The module compiles and formatting/truncation behave.)

---

## Task 2: `tray.rs` — menu rows + `set_last_message`

**Files:**
- Modify: `src-tauri/src/tray.rs`

- [ ] **Step 1: Update imports**

In `src-tauri/src/tray.rs`, replace the top two `use` lines:

```rust
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};
```

with:

```rust
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};
```

- [ ] **Step 2: Extend `TrayMenuItems`**

Replace the existing struct (currently only `show_hide`) with:

```rust
/// Menu items we update after creation. The Show/Hide label mirrors the unread
/// count because tray-icon's GTK backend makes tooltips a no-op on Linux. The
/// last-message rows are detached at first and inserted at the top on the first
/// incoming message (MenuItem has no set_visible in tauri 2.11).
pub struct TrayMenuItems {
    pub show_hide: MenuItem<tauri::Wry>,
    pub menu: Menu<tauri::Wry>,
    pub last_from: MenuItem<tauri::Wry>,
    pub last_body: MenuItem<tauri::Wry>,
    pub separator: PredefinedMenuItem<tauri::Wry>,
    pub last_inserted: AtomicBool,
}
```

- [ ] **Step 3: Build the rows in `create` and store them**

In `create`, after the existing `let menu = Menu::with_items(app, &[&show_hide, &reload, &clear, &quit])?;` line, and before `TrayIconBuilder`, add:

```rust
    // Detached now; inserted at the top of `menu` on the first message.
    let last_from = MenuItem::with_id(app, "last_from", "", false, None::<&str>)?;
    let last_body = MenuItem::with_id(app, "last_body", "", false, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
```

Then replace the final `app.manage(TrayMenuItems { show_hide });` with:

```rust
    app.manage(TrayMenuItems {
        show_hide,
        menu,
        last_from,
        last_body,
        separator,
        last_inserted: AtomicBool::new(false),
    });
```

Note: `menu` is moved into `TrayMenuItems` after `TrayIconBuilder::...menu(&menu)...build(app)?` has already borrowed it, so no clone is needed. Leave the `TrayIconBuilder` block between the `let separator = ...` line and the `app.manage(...)` call unchanged.

- [ ] **Step 4: Add `set_last_message`**

Add this function below `set_unread` in `src-tauri/src/tray.rs`:

```rust
/// Update the tray's "last message" rows. On the first call the rows + a
/// separator are inserted at the top of the menu (MenuItem has no set_visible).
/// GTK menu mutation must run on the main thread; a failure here is cosmetic and
/// must never break notification delivery.
pub fn set_last_message(app: &AppHandle, from: &str, ts: &str, message: &str) {
    let from_label = crate::last_message::from_label(from, ts);
    let body_label = crate::last_message::body_label(message);
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(items) = handle.try_state::<TrayMenuItems>() {
            let _ = items.last_from.set_text(&from_label);
            let _ = items.last_body.set_text(&body_label);
            if !items.last_inserted.swap(true, Ordering::AcqRel) {
                let _ = items.menu.insert(&items.last_from, 0);
                let _ = items.menu.insert(&items.last_body, 1);
                let _ = items.menu.insert(&items.separator, 2);
            }
        }
    });
}
```

- [ ] **Step 5: Verify it compiles**

Run: `cd src-tauri && cargo build`
Expected: clean build. (No unit test here — GTK menu behavior is verified live in Task 5.)
If a `Send`/`Sync` error appears for the new `TrayMenuItems` fields, STOP and report it — the design assumes `Menu`/`PredefinedMenuItem<Wry>` are `Send + Sync` like the already-stored `MenuItem<Wry>`.

---

## Task 3: `deliver_notification` + shim send the timestamp

**Files:**
- Modify: `src-tauri/src/notifications.rs`
- Modify: `src-tauri/inject/notification-shim.js`

- [ ] **Step 1: Add `ts` to the command and call `set_last_message`**

In `src-tauri/src/notifications.rs`, change the `deliver_notification` signature and add the call at the very top of the body (before the existing `std::thread::spawn(...)`):

```rust
#[tauri::command]
pub fn deliver_notification(app: AppHandle, title: String, body: String, ts: String) {
    crate::tray::set_last_message(&app, &title, &ts, &body);
    std::thread::spawn(move || {
```

Leave the rest of the function body (the thread that shows the native notification with the "default" action) exactly as it is. The command name is unchanged, so `build.rs`, `invoke_handler!`, and `capabilities/remote.json` need no edits.

- [ ] **Step 2: Send `ts` from the shim**

In `src-tauri/inject/notification-shim.js`, add a `fmtTs` helper inside the IIFE (e.g. just above `function FakeNotification`):

```js
  // Smart local timestamp: "14:32" today, else "Jun 13 14:32".
  function fmtTs(d) {
    var now = new Date();
    var sameDay =
      d.getFullYear() === now.getFullYear() &&
      d.getMonth() === now.getMonth() &&
      d.getDate() === now.getDate();
    var time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    if (sameDay) return time;
    var date = d.toLocaleDateString([], { month: "short", day: "numeric" });
    return date + " " + time;
  }
```

Then change the `send(...)` call inside `FakeNotification` to include `ts`:

```js
  function FakeNotification(title, opts) {
    opts = opts || {};
    send("deliver_notification", {
      title: String(title || ""),
      body: String(opts.body || ""),
      ts: fmtTs(new Date()),
    });
  }
```

- [ ] **Step 3: Verify the workspace still builds and all tests pass**

Run: `cd src-tauri && cargo test`
Expected: builds clean; all tests pass (the new `last_message::` tests plus the existing `badge`, `tray`, `unread` tests).

---

## Task 4: Version bump for release-on-merge

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Bump `Cargo.toml`**

In `src-tauri/Cargo.toml`, change the package version:

```toml
version = "0.2.0"
```

- [ ] **Step 2: Bump `tauri.conf.json`**

In `src-tauri/tauri.conf.json`, change the version to match:

```json
  "version": "0.2.0",
```

- [ ] **Step 3: Refresh the lockfile and confirm versions match**

Run: `cd src-tauri && cargo build` (updates `Cargo.lock`'s `whatstauri` entry to `0.2.0`), then:

Run: `grep -m1 '^version' src-tauri/Cargo.toml; grep -m1 '"version"' src-tauri/tauri.conf.json`
Expected: both report `0.2.0` (the CI version-consistency job requires this; the mismatch fails the build).

---

## Task 5: Gates and live verification

**Files:** none (verification only)

- [ ] **Step 1: Run the CI gates**

```bash
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
Expected: fmt prints nothing, clippy emits no warnings, all tests pass.

- [ ] **Step 2: Live-check the tray rows**

Run: `cd src-tauri && cargo tauri dev`

To see the rows without waiting for a real message, temporarily add this line at the end of the `setup` closure in `src-tauri/src/lib.rs` (after `tray::create(app.handle())?;`):

```rust
tray::set_last_message(app.handle(), "Alice", "14:32", "see you at 5pm"); // TEMP: visual check, revert before commit
```

Rebuild, open the tray menu, and confirm: a greyed **`Alice · 14:32`** row, a greyed **`“see you at 5pm”`** row, then a separator above **Show/Hide WhatsApp**. **Revert this line** before finishing.

- [ ] **Step 3: Confirm the throwaway line is gone**

Run: `git diff -- src-tauri/src/lib.rs`
Expected: the diff shows only `mod last_message;` added — no `set_last_message(...,"Alice",...)` TEMP line. If the TEMP line is still present, remove it.

- [ ] **Step 4: Final build after revert**

Run: `cd src-tauri && cargo build`
Expected: clean build with no TEMP code.

---

## Notes

- **No IPC command added/renamed** → no `build.rs` / `invoke_handler!` / `capabilities/remote.json` / `gen/` changes. Only `deliver_notification`'s argument list grew, which the shim's payload keys (`title`, `body`, `ts`) must match — they do.
- **No new crate dependency.**
- The `0.2.0` bump (Task 4) is what makes `tag-on-version.yml` cut a release when `icon-update` merges to `main`; it also covers the tray-badge feature already on this branch.
- Leave all changes unstaged for the user to review and commit (git hook blocks commits).
