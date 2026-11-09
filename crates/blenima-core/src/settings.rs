//! The user's preferences, kept in `settings.toml` in the OS's config folder.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::tomlfile;

/// Smallest text size of the views, in pixels.
pub const FONT_SIZE_MIN: u8 = 9;
/// Largest text size of the views, in pixels.
pub const FONT_SIZE_MAX: u8 = 24;
/// Text size of the views for a new install, in pixels.
pub const FONT_SIZE_DEFAULT: u8 = 13;

/// Written at the top of the file, so someone who opens it knows what it is.
const HEADER: &str = "# bleNIMA settings. Edit them while bleNIMA is closed.\n\n";

/// Which colors the app uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Dark or light, following the OS.
    #[default]
    System,
    Dark,
    Light,
}

/// What comes back when bleNIMA starts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StartupSettings {
    /// Opens the tabs from last time.
    pub restore_tabs: bool,
    /// Connects the tabs that were connected when bleNIMA closed.
    pub reconnect: bool,
    /// Opens the window where it was, at the size it had.
    pub restore_window: bool,
}

impl Default for StartupSettings {
    fn default() -> Self {
        Self {
            restore_tabs: true,
            reconnect: true,
            restore_window: true,
        }
    }
}

/// Looking on GitHub for a newer bleNIMA.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    /// Looks when bleNIMA starts.
    pub check: bool,
    /// A version not to tell about again, such as "1.0.2"; empty for none.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub skipped: String,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            check: true,
            skipped: String::new(),
        }
    }
}

/// Everything in `settings.toml`. A value missing from the file takes its default.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    /// The text size of the views: lists, values and terminals.
    pub font_size: u8,
    pub startup: StartupSettings,
    pub updates: UpdateSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            font_size: FONT_SIZE_DEFAULT,
            startup: StartupSettings::default(),
            updates: UpdateSettings::default(),
        }
    }
}

impl Settings {
    /// Reads the settings from `path`; see [`tomlfile::load`] for missing and broken files.
    pub fn load(path: &Path) -> Self {
        tomlfile::load::<Settings>(path).checked()
    }

    /// Writes the settings to `path`, safely; see [`tomlfile::save`].
    pub fn save(&self, path: &Path) -> io::Result<()> {
        tomlfile::save(self, path, HEADER)
    }

    /// The same settings with every value moved into its allowed range.
    pub fn checked(mut self) -> Self {
        self.font_size = self.font_size.clamp(FONT_SIZE_MIN, FONT_SIZE_MAX);
        self
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::tomlfile::tests::TempDir;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = TempDir::new("settings-missing");
        assert_eq!(
            Settings::load(&dir.file("settings.toml")),
            Settings::default()
        );
    }

    #[test]
    fn saved_settings_load_back_the_same() {
        let dir = TempDir::new("settings-round-trip");
        let path = dir.file("nested/folder/settings.toml");
        let settings = Settings {
            theme: Theme::Light,
            font_size: 18,
            startup: StartupSettings {
                restore_tabs: true,
                reconnect: false,
                restore_window: false,
            },
            updates: UpdateSettings {
                check: false,
                skipped: "1.0.2".into(),
            },
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
        assert!(!path.with_extension("toml.tmp").exists());
    }

    #[test]
    fn saved_file_is_readable_toml() {
        let dir = TempDir::new("settings-readable");
        let path = dir.file("settings.toml");
        Settings::default().save(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# bleNIMA settings"));
        assert!(text.contains("theme = \"system\""));
        assert!(text.contains("font_size = 13"));
    }

    #[test]
    fn missing_values_take_their_defaults() {
        let dir = TempDir::new("settings-partial");
        let path = dir.file("settings.toml");
        fs::write(&path, "theme = \"dark\"\n").unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.theme, Theme::Dark);
        assert_eq!(settings.font_size, FONT_SIZE_DEFAULT);
        assert!(settings.startup.restore_tabs && settings.startup.reconnect);
        assert!(settings.updates.check);
        assert_eq!(settings.updates.skipped, "");
    }

    #[test]
    fn unknown_values_are_ignored() {
        let dir = TempDir::new("settings-unknown");
        let path = dir.file("settings.toml");
        fs::write(&path, "theme = \"light\"\nfrom_the_future = 1\n").unwrap();
        assert_eq!(Settings::load(&path).theme, Theme::Light);
    }

    #[test]
    fn broken_file_gives_defaults_and_is_kept() {
        let dir = TempDir::new("settings-broken");
        let path = dir.file("settings.toml");
        fs::write(&path, "theme = \"purple\"\n").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        let kept = fs::read_to_string(dir.file("settings.bad.toml")).unwrap();
        assert_eq!(kept, "theme = \"purple\"\n");
    }

    #[test]
    fn font_size_is_kept_in_range() {
        let small = Settings {
            font_size: 2,
            ..Settings::default()
        };
        let large = Settings {
            font_size: 200,
            ..Settings::default()
        };
        assert_eq!(small.checked().font_size, FONT_SIZE_MIN);
        assert_eq!(large.checked().font_size, FONT_SIZE_MAX);
    }
}
