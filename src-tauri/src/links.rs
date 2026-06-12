/// In-app navigation allowlist (spec §3.1): *.whatsapp.com / *.whatsapp.net, https only.
pub(crate) fn is_internal(url: &url::Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    matches!(url.host_str(), Some(h) if h == "whatsapp.com"
        || h.ends_with(".whatsapp.com")
        || h == "whatsapp.net"
        || h.ends_with(".whatsapp.net"))
}

/// The "browser not supported" wall (spec §5.1).
pub(crate) fn is_browser_wall(url: &url::Url) -> bool {
    is_internal(url) && url.path() == "/browsers.html"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> url::Url {
        url::Url::parse(s).unwrap()
    }

    #[test]
    fn whatsapp_domains_stay_internal() {
        assert!(is_internal(&u("https://web.whatsapp.com/")));
        assert!(is_internal(&u("https://whatsapp.com/x")));
        assert!(is_internal(&u("https://static.whatsapp.net/rsrc.php/x.js")));
        assert!(is_internal(&u("https://whatsapp.net/")));
    }

    #[test]
    fn lookalikes_and_other_hosts_are_external() {
        assert!(!is_internal(&u("https://evil-whatsapp.com/")));
        assert!(!is_internal(&u("https://whatsapp.com.evil.com/")));
        assert!(!is_internal(&u("https://google.com/")));
        assert!(!is_internal(&u("https://web.whatsapp.com./"))); // trailing-dot FQDN must not match the suffix check
    }

    #[test]
    fn non_https_is_external() {
        assert!(!is_internal(&u("http://web.whatsapp.com/")));
    }

    #[test]
    fn detects_unsupported_browser_page() {
        assert!(is_browser_wall(&u(
            "https://web.whatsapp.com/browsers.html?missing=x"
        )));
        assert!(!is_browser_wall(&u("https://web.whatsapp.com/")));
    }
}
