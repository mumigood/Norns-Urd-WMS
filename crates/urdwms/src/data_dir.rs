//! Shared location for desktop, API and migration of existing egui installs.
use std::{ffi::OsString, path::PathBuf};
fn choose(explicit: Option<OsString>, local: Option<OsString>, home: Option<OsString>, windows: bool) -> PathBuf {
    if let Some(path) = explicit {
        return path.into();
    }
    if windows && let Some(local) = local {
        return PathBuf::from(local).join("UrdWMS");
    }
    home.map(|h| PathBuf::from(h).join(".local/share/urdwms")).unwrap_or_else(|| PathBuf::from("urdwms-data"))
}
pub fn default_data_dir() -> PathBuf {
    choose(std::env::var_os("URDWMS_DATA"), std::env::var_os("LOCALAPPDATA"), std::env::var_os("HOME"), cfg!(windows))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn integrated_host_reuses_original_desktop_directory() {
        assert_eq!(
            choose(None, Some("C:/Users/test/AppData/Local".into()), None, true),
            PathBuf::from("C:/Users/test/AppData/Local/UrdWMS")
        );
        assert_eq!(
            choose(Some("D:/existing warehouse".into()), Some("ignored".into()), None, true),
            PathBuf::from("D:/existing warehouse")
        );
        assert_eq!(
            choose(None, None, Some("/home/example".into()), false),
            PathBuf::from("/home/example/.local/share/urdwms")
        );
    }
}
