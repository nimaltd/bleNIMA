//! What is open in the window, kept in `workspace.toml`, so the app opens the
//! way it was left: the same tabs, in the same window.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::tomlfile;

const HEADER: &str =
    "# bleNIMA workspace: the tabs that were open. bleNIMA writes it on every change.\n\n";

/// The smallest window size kept, inside its frame. A smaller one is from a
/// window that was minimized or not drawn yet, not one the user made.
const WINDOW_MIN: (u32, u32) = (320, 200);

/// What a tab is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabKind {
    /// The devices around, from their adverts.
    #[default]
    Scanner,
    /// One device, connected: its GATT table, values and tests.
    Device,
    /// A device as a terminal: NUS, a pair of characteristics, L2CAP or SPP.
    Terminal,
    /// bleNIMA as a device: an advertiser and a GATT server.
    Simulator,
    /// Every packet on the air, through a sniffer dongle.
    Sniffer,
    /// A Bluetooth mesh network.
    Mesh,
    /// LE Audio: Auracast broadcasts and their streams.
    Audio,
    /// Angles from the constant tone extension's IQ samples.
    DirectionFinding,
    /// Distance between two nRF54L15s.
    ChannelSounding,
    /// Any HCI command to the dongle, and every event back.
    HciConsole,
}

/// What Enter sends in a terminal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineEnding {
    #[default]
    Cr,
    Lf,
    CrLf,
}

/// How a terminal talks to its device: which characteristic it writes, and
/// which one it hears.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalSetup {
    /// The key of the characteristic written to: "service/characteristic".
    pub write: String,
    /// The key of the characteristic whose notifications are shown.
    pub notify: String,
    pub enter: LineEnding,
    /// Shows what is typed, for devices that do not echo it back.
    pub local_echo: bool,
}

/// How a scanner tab filters and sorts its devices.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanFilter {
    /// Text the name, address, company or a service has in it.
    pub text: String,
    /// The weakest RSSI shown, in dBm.
    pub min_rssi: i16,
    pub only_named: bool,
    pub only_connectable: bool,
    pub only_favourites: bool,
    /// "rssi", "name", "last" or "first".
    pub sort: String,
}

impl Default for ScanFilter {
    fn default() -> Self {
        Self {
            text: String::new(),
            min_rssi: -100,
            only_named: false,
            only_connectable: false,
            only_favourites: false,
            sort: "rssi".into(),
        }
    }
}

/// One tab, as it is saved.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabSetup {
    pub name: String,
    pub kind: TabKind,
    /// Whether it was connected, or scanning, when it was saved.
    pub connected: bool,
    /// The device a device or terminal tab connects to: the radio's id for it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub device: String,
    /// What the tab shows: "list", "plot", "gatt", "terminal", "log", "tools".
    #[serde(skip_serializing_if = "String::is_empty")]
    pub view: String,
    /// A scanner's filters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<ScanFilter>,
    /// The devices a scanner keeps at the top and plots.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub favourites: Vec<String>,
    /// A device tab's terminal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<TerminalSetup>,
    /// The format each characteristic's value is shown in, by key: "u16", "utf8".
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub formats: std::collections::BTreeMap<String, String>,
}

/// The window's place and size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowState {
    /// The window's top left corner, its frame included.
    pub x: i32,
    pub y: i32,
    /// The size of the window's inside, its frame not included.
    pub width: u32,
    pub height: u32,
    /// Whether it filled the screen. The place and size above are then the
    /// ones it goes back to.
    #[serde(default)]
    pub maximized: bool,
}

/// The part of a screen windows can use, without the taskbar or the Dock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl WindowState {
    /// Whether enough of the window's title bar is on one of `screens` to
    /// grab it, so a window whose screen was unplugged is not put out of reach.
    pub fn reachable(&self, screens: &[Area]) -> bool {
        // The top 30 pixels of the window: at least 100 by 20 of them on one screen.
        screens.iter().any(|screen| {
            let left = self.x.max(screen.x);
            let right = (self.x + self.width as i32).min(screen.x + screen.width as i32);
            let top = self.y.max(screen.y);
            let bottom = (self.y + 30).min(screen.y + screen.height as i32);
            right - left >= 100 && bottom - top >= 20
        })
    }
}

/// Everything in `workspace.toml`. A value missing from the file takes its default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    /// The tab in front, counted from 0.
    pub active: usize,
    /// The window's place and size when it last moved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowState>,
    pub tabs: Vec<TabSetup>,
}

impl Workspace {
    /// Reads the workspace from `path`; see [`tomlfile::load`] for missing and broken files.
    pub fn load(path: &Path) -> Self {
        tomlfile::load::<Workspace>(path).checked()
    }

    /// Writes the workspace to `path`, safely; see [`tomlfile::save`].
    pub fn save(&self, path: &Path) -> io::Result<()> {
        tomlfile::save(self, path, HEADER)
    }

    /// The same workspace with the tab in front pointing at a tab that
    /// exists, and without a window size too small to be real.
    pub fn checked(mut self) -> Self {
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        self.window = self
            .window
            .filter(|window| window.width >= WINDOW_MIN.0 && window.height >= WINDOW_MIN.1);
        self
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::tomlfile::tests::TempDir;

    fn tab(name: &str, kind: TabKind) -> TabSetup {
        TabSetup {
            name: name.into(),
            kind,
            ..TabSetup::default()
        }
    }

    #[test]
    fn missing_file_gives_an_empty_workspace() {
        let dir = TempDir::new("workspace-missing");
        assert_eq!(
            Workspace::load(&dir.file("workspace.toml")),
            Workspace::default()
        );
    }

    #[test]
    fn saved_workspace_loads_back_the_same() {
        let dir = TempDir::new("workspace-round-trip");
        let path = dir.file("workspace.toml");
        let workspace = Workspace {
            active: 1,
            window: Some(WindowState {
                x: -1200,
                y: 40,
                width: 1200,
                height: 760,
                maximized: true,
            }),
            tabs: vec![
                tab("Scanner", TabKind::Scanner),
                TabSetup {
                    connected: true,
                    device: "C3:1A:4B:00:12:9F".into(),
                    view: "terminal".into(),
                    terminal: Some(TerminalSetup {
                        write: "a/b".into(),
                        notify: "a/c".into(),
                        enter: LineEnding::CrLf,
                        local_echo: true,
                    }),
                    formats: [("a/c".to_string(), "u16".to_string())].into(),
                    ..tab("nrf52 shell", TabKind::Terminal)
                },
                TabSetup {
                    filter: Some(ScanFilter {
                        text: "nrf".into(),
                        min_rssi: -70,
                        ..ScanFilter::default()
                    }),
                    favourites: vec!["C3:1A:4B:00:12:9F".into()],
                    ..tab("Near", TabKind::Scanner)
                },
                tab("Range test", TabKind::ChannelSounding),
            ],
        };
        workspace.save(&path).unwrap();
        assert_eq!(Workspace::load(&path), workspace);
    }

    #[test]
    fn kinds_are_written_as_words() {
        let dir = TempDir::new("workspace-words");
        let path = dir.file("workspace.toml");
        let workspace = Workspace {
            tabs: vec![
                tab("Angles", TabKind::DirectionFinding),
                tab("Raw", TabKind::HciConsole),
            ],
            ..Workspace::default()
        };
        workspace.save(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# bleNIMA workspace"));
        assert!(text.contains("kind = \"direction_finding\""));
        assert!(text.contains("kind = \"hci_console\""));
    }

    #[test]
    fn the_tab_in_front_is_one_that_exists() {
        let workspace = Workspace {
            active: 7,
            tabs: vec![tab("A", TabKind::Scanner), tab("B", TabKind::Device)],
            ..Workspace::default()
        };
        assert_eq!(workspace.checked().active, 1);
        let empty = Workspace {
            active: 3,
            ..Workspace::default()
        };
        assert_eq!(empty.checked().active, 0);
    }

    #[test]
    fn a_window_too_small_to_be_real_is_dropped() {
        let workspace = Workspace {
            window: Some(WindowState {
                x: 0,
                y: 0,
                width: 160,
                height: 28,
                maximized: false,
            }),
            ..Workspace::default()
        };
        assert_eq!(workspace.checked().window, None);
    }

    #[test]
    fn a_window_is_reachable_only_with_its_title_bar_on_a_screen() {
        let screen = Area {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        };
        let window = WindowState {
            x: 100,
            y: 100,
            width: 1200,
            height: 760,
            maximized: false,
        };
        assert!(window.reachable(&[screen]));
        // On a screen to the left that is not there any more.
        let gone = WindowState { x: -1500, ..window };
        assert!(!gone.reachable(&[screen]));
        // Only its bottom edge shows at the top of the screen.
        let above = WindowState { y: -740, ..window };
        assert!(!above.reachable(&[screen]));
    }
}
