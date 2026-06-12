use serde::Deserialize;
use std::path::PathBuf;

/// Bump the Chrome version here each release (spec §6.3 "UA rot").
const DEFAULT_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub user_agent: String,
    pub close_to_tray: bool,
    pub start_hidden: bool,
    /// Sets WEBKIT_DISABLE_DMABUF_RENDERER=1 before webview init (NVIDIA/Wayland glitches).
    pub disable_dmabuf_renderer: bool,
    /// Sets WEBKIT_DISABLE_COMPOSITING_MODE=1 before webview init.
    pub disable_compositing: bool,
    /// Sets WEBKIT_DMABUF_RENDERER_FORCE_SHM=1 — GPU painting with shared-memory
    /// transport; compatibility middle ground between full GPU and software rendering.
    pub force_shm: bool,
    /// Auto-apply NVIDIA workarounds (explicit-sync off on Wayland, NVDEC demoted)
    /// when an NVIDIA GPU is detected. Set false to manage the env yourself.
    pub nvidia_quirks: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            user_agent: DEFAULT_UA.to_string(),
            close_to_tray: true,
            start_hidden: false,
            disable_dmabuf_renderer: false,
            disable_compositing: false,
            force_shm: false,
            nvidia_quirks: true,
        }
    }
}

impl Config {
    pub fn from_toml(s: &str) -> Self {
        toml::from_str(s).unwrap_or_else(|e| {
            eprintln!("whatstauri: invalid config, using defaults: {e}");
            Self::default()
        })
    }

    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("whatstauri").join("config.toml"))
    }

    /// Missing file → defaults silently. Unreadable file → warn + defaults.
    /// Never refuses to start (spec §3.4).
    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(s) => Self::from_toml(&s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => {
                eprintln!("whatstauri: cannot read config {}: {e}", path.display());
                Self::default()
            }
        }
    }

    /// Must run BEFORE any GTK/WebKit initialization.
    // NOTE: set_var is safe in edition 2021 (becomes `unsafe` in edition 2024);
    // called from the main thread before GTK/WebKit spawns threads either way.
    pub fn apply_env_workarounds(&self) {
        if self.disable_dmabuf_renderer {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
        if self.disable_compositing {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        }
        if self.force_shm {
            std::env::set_var("WEBKIT_DMABUF_RENDERER_FORCE_SHM", "1");
        }
        if self.nvidia_quirks && detect_nvidia() {
            // WebKit bug 280210: NVIDIA explicit-sync crashes WebKitGTK on Wayland
            // (Error 71). Disabling it keeps the full GPU path. Respect existing env.
            if std::env::var_os("WAYLAND_DISPLAY").is_some()
                && std::env::var_os("__NV_DISABLE_EXPLICIT_SYNC").is_none()
            {
                std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1");
            }
            // NVDEC is unreliable inside WebKit's sandbox (missing nvrtc); demote it
            // so GStreamer picks openh264 (verified working for WhatsApp video).
            if std::env::var_os("GST_PLUGIN_FEATURE_RANK").is_none() {
                std::env::set_var(
                    "GST_PLUGIN_FEATURE_RANK",
                    "nvh264dec:NONE,nvh265dec:NONE,nvav1dec:NONE",
                );
            }
        }
    }
}

/// True if any DRM device vendor id is NVIDIA (0x10de).
fn is_nvidia_vendor_list(vendors: &[String]) -> bool {
    vendors
        .iter()
        .any(|v| v.trim().eq_ignore_ascii_case("0x10de"))
}

fn detect_nvidia() -> bool {
    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return false;
    };
    let vendors: Vec<String> = entries
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path().join("device/vendor")).ok())
        .collect();
    is_nvidia_vendor_list(&vendors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let c = Config::default();
        assert!(c.user_agent.contains("Chrome/"));
        assert!(c.user_agent.contains("Linux"));
        assert!(c.close_to_tray);
        assert!(!c.start_hidden);
        assert!(!c.disable_dmabuf_renderer);
        assert!(!c.disable_compositing);
    }

    #[test]
    fn parses_full_toml() {
        let c = Config::from_toml(
            r#"
            user_agent = "UA-TEST"
            close_to_tray = false
            start_hidden = true
            disable_dmabuf_renderer = true
            disable_compositing = true
            "#,
        );
        assert_eq!(c.user_agent, "UA-TEST");
        assert!(!c.close_to_tray);
        assert!(c.start_hidden);
        assert!(c.disable_dmabuf_renderer);
        assert!(c.disable_compositing);
    }

    #[test]
    fn partial_toml_fills_defaults() {
        let c = Config::from_toml(r#"start_hidden = true"#);
        assert!(c.start_hidden);
        assert!(c.close_to_tray); // default preserved
        assert!(c.user_agent.contains("Chrome/"));
    }

    #[test]
    fn malformed_toml_falls_back_to_defaults() {
        let c = Config::from_toml("this is {{{ not toml");
        assert_eq!(c.user_agent, Config::default().user_agent);
    }

    #[test]
    fn new_fields_default_correctly() {
        let c = Config::default();
        assert!(!c.force_shm);
        assert!(c.nvidia_quirks);
    }

    #[test]
    fn nvidia_vendor_detection() {
        assert!(is_nvidia_vendor_list(&["0x10de\n".to_string()]));
        assert!(is_nvidia_vendor_list(&[
            "0x8086\n".to_string(),
            "0x10DE\n".to_string()
        ]));
        assert!(!is_nvidia_vendor_list(&[
            "0x8086\n".to_string(),
            "0x1002\n".to_string()
        ]));
        assert!(!is_nvidia_vendor_list(&[]));
    }

    #[test]
    fn parses_new_toml_fields() {
        let c = Config::from_toml("force_shm = true\nnvidia_quirks = false");
        assert!(c.force_shm);
        assert!(!c.nvidia_quirks);
    }
}
