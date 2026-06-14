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
