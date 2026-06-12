/// Extract unread count from WhatsApp Web's document title ("(3) WhatsApp").
/// Unrecognized titles return `previous` — never wipe a real count on noise.
pub(crate) fn parse_unread(title: &str, previous: u32) -> u32 {
    let trimmed = title.trim();
    if let Some(rest) = trimmed.strip_prefix('(') {
        if let Some(end) = rest.find(')') {
            if let Ok(n) = rest[..end].parse::<u32>() {
                return n;
            }
        }
        return previous;
    }
    if trimmed.contains("WhatsApp") {
        0
    } else {
        previous
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_prefix_wins() {
        assert_eq!(parse_unread("(3) WhatsApp", 0), 3);
        assert_eq!(parse_unread("(12) WhatsApp", 5), 12);
        assert_eq!(parse_unread("(3)", 7), 3);
        assert_eq!(parse_unread("(0) WhatsApp", 5), 0);
    }

    #[test]
    fn plain_whatsapp_title_means_zero() {
        assert_eq!(parse_unread("WhatsApp", 4), 0);
        assert_eq!(parse_unread("  WhatsApp  ", 4), 0);
    }

    #[test]
    fn unrecognized_title_keeps_previous() {
        // A flaky/loading title must never wipe a real count (spec §3.3).
        assert_eq!(parse_unread("", 4), 4);
        assert_eq!(parse_unread("Connecting…", 4), 4);
        assert_eq!(parse_unread("(abc) WhatsApp", 2), 2);
        assert_eq!(parse_unread("(", 9), 9);
    }
}
