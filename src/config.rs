use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use toml;

use dirs;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    #[serde(default = "default_switch_mode")]
    pub switch: char,

    #[serde(default = "default_device_mode")]
    pub mode: String,

    #[serde(default = "default_esc_quit")]
    pub esc_quit: bool,

    #[serde(default = "default_vpn")]
    pub vpn: char,

    /// How often to re-read NetworkManager state, in milliseconds. Raising this
    /// trades how quickly changes made outside wlctl show up for less work per
    /// second on machines with many saved profiles.
    #[serde(default = "default_refresh_interval_ms")]
    pub refresh_interval_ms: u64,

    #[serde(default)]
    pub device: Device,

    #[serde(default)]
    pub station: Station,

    #[serde(default)]
    pub ap: AccessPoint,
}

fn default_switch_mode() -> char {
    'r'
}

fn default_device_mode() -> String {
    "station".to_string()
}

fn default_esc_quit() -> bool {
    false
}

fn default_vpn() -> char {
    'v'
}

fn default_refresh_interval_ms() -> u64 {
    1_000
}

/// Floor for [`Config::refresh_interval`]. A mistyped interval should slow the
/// UI down, never turn it into a busy loop.
const MIN_REFRESH_INTERVAL_MS: u64 = 250;

// Device
#[derive(Deserialize, Debug)]
pub struct Device {
    #[serde(default = "default_show_device_infos")]
    pub infos: char,
    #[serde(default = "default_toggle_power")]
    pub toggle_power: char,
    #[serde(default = "default_doctor")]
    pub doctor: char,
}

impl Default for Device {
    fn default() -> Self {
        Self {
            infos: default_show_device_infos(),
            toggle_power: default_toggle_power(),
            doctor: default_doctor(),
        }
    }
}

fn default_show_device_infos() -> char {
    'i'
}

fn default_toggle_power() -> char {
    'o'
}

fn default_doctor() -> char {
    '?'
}

// Station
#[derive(Deserialize, Debug)]
pub struct Station {
    #[serde(default = "default_station_start_scanning")]
    pub start_scanning: char,

    #[serde(default)]
    pub known_network: KnownNetwork,

    #[serde(default)]
    pub new_network: NewNetwork,
}

impl Default for Station {
    fn default() -> Self {
        Self {
            start_scanning: default_station_start_scanning(),
            known_network: KnownNetwork::default(),
            new_network: NewNetwork::default(),
        }
    }
}

fn default_station_start_scanning() -> char {
    's'
}

#[derive(Deserialize, Debug)]
pub struct KnownNetwork {
    #[serde(default = "default_station_remove_known_network")]
    pub remove: char,
    pub toggle_autoconnect: char,
    pub show_all: char,
    pub share: char,
    #[serde(default = "default_station_speed_test")]
    pub speed_test: char,
    #[serde(default = "default_station_prefer")]
    pub prefer: char,
}

impl Default for KnownNetwork {
    fn default() -> Self {
        Self {
            remove: 'd',
            toggle_autoconnect: 't',
            show_all: 'a',
            share: 'p',
            speed_test: 'S',
            prefer: 'u',
        }
    }
}

fn default_station_prefer() -> char {
    'u'
}

fn default_station_speed_test() -> char {
    'S'
}

fn default_station_remove_known_network() -> char {
    'd'
}

#[derive(Deserialize, Debug)]
pub struct NewNetwork {
    pub show_all: char,
    #[serde(default = "default_connect_hidden")]
    pub connect_hidden: char,
    #[serde(default = "default_new_network_filter")]
    pub filter: char,
}

impl Default for NewNetwork {
    fn default() -> Self {
        Self {
            show_all: 'a',
            connect_hidden: 'h',
            filter: '/',
        }
    }
}

fn default_connect_hidden() -> char {
    'h'
}

fn default_new_network_filter() -> char {
    '/'
}

// Access Point
#[derive(Deserialize, Debug)]
pub struct AccessPoint {
    #[serde(default = "default_ap_start")]
    pub start: char,

    #[serde(default = "default_ap_stop")]
    pub stop: char,
}

impl Default for AccessPoint {
    fn default() -> Self {
        Self {
            start: default_ap_start(),
            stop: default_ap_stop(),
        }
    }
}

fn default_ap_start() -> char {
    'n'
}

fn default_ap_stop() -> char {
    'x'
}

impl Config {
    /// How often to re-read NetworkManager state.
    pub fn refresh_interval(&self) -> Duration {
        Duration::from_millis(self.refresh_interval_ms.max(MIN_REFRESH_INTERVAL_MS))
    }

    /// Read the user's config, falling back to the built-in defaults.
    pub fn load() -> (Self, Option<ConfigError>) {
        let Some(conf_dir) = dirs::config_dir() else {
            return (Self::default(), Some(ConfigError::NoConfigDirectory));
        };

        let path = conf_dir.join("wlctl").join("config.toml");

        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return (Self::default(), None),
            Err(source) => {
                return (
                    Self::default(),
                    Some(ConfigError::Unreadable { path, source }),
                );
            }
        };

        parse_config(&text, &path)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            switch: default_switch_mode(),
            mode: default_device_mode(),
            esc_quit: default_esc_quit(),
            vpn: default_vpn(),
            refresh_interval_ms: default_refresh_interval_ms(),
            device: Device::default(),
            station: Station::default(),
            ap: AccessPoint::default(),
        }
    }
}

/// Why [`Config::load`] fell back to the built-in defaults.
#[derive(Debug)]
pub enum ConfigError {
    /// There is no config directory to look in.
    NoConfigDirectory,
    /// The file is there but could not be read.
    Unreadable { path: PathBuf, source: io::Error },
    /// The file was read but is not a valid config.
    Invalid {
        path: PathBuf,
        /// One-based line the parser objected to, when it reported a position.
        line: Option<usize>,
        source: toml::de::Error,
    },
}

impl ConfigError {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::NoConfigDirectory => None,
            Self::Unreadable { path, .. } | Self::Invalid { path, .. } => Some(path),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoConfigDirectory => write!(f, "No config directory to read."),
            Self::Unreadable { source, .. } => write!(f, "Cannot read config: {source}"),
            Self::Invalid { line, source, .. } => match line {
                Some(line) => write!(f, "Invalid config, line {line}: {}", source.message()),
                None => write!(f, "Invalid config: {}", source.message()),
            },
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoConfigDirectory => None,
            Self::Unreadable { source, .. } => Some(source),
            Self::Invalid { source, .. } => Some(source),
        }
    }
}

fn parse_config(text: &str, path: &Path) -> (Config, Option<ConfigError>) {
    match toml::from_str(text) {
        Ok(config) => (config, None),
        Err(source) => (
            Config::default(),
            Some(ConfigError::Invalid {
                path: path.to_path_buf(),
                line: error_line(text, &source),
                source,
            }),
        ),
    }
}

fn error_line(text: &str, error: &toml::de::Error) -> Option<usize> {
    let consumed = text.get(..error.span()?.start)?;
    Some(consumed.bytes().filter(|b| *b == b'\n').count() + 1)
}

#[cfg(test)]
mod tests {
    use super::{Config, ConfigError, error_line, parse_config};
    use serde::de::Error as _;
    use std::path::Path;

    fn parse(text: &str) -> (Config, Option<ConfigError>) {
        parse_config(text, Path::new("config.toml"))
    }

    /// The fallback hands back [`Config::default`], so every field has to carry
    /// a serde default for a config file to stay optional. A field added
    /// without one breaks this test rather than someone's startup.
    #[test]
    fn empty_config_matches_the_built_in_defaults() {
        let (config, error) = parse("");
        let defaults = Config::default();

        assert!(error.is_none());
        assert_eq!(config.refresh_interval_ms, defaults.refresh_interval_ms);
        assert_eq!(config.switch, defaults.switch);
        assert_eq!(config.mode, defaults.mode);
        assert_eq!(config.esc_quit, defaults.esc_quit);
        assert_eq!(config.vpn, defaults.vpn);
        assert_eq!(config.device.infos, defaults.device.infos);
        assert_eq!(
            config.station.start_scanning,
            defaults.station.start_scanning
        );
        assert_eq!(config.ap.start, defaults.ap.start);
    }

    #[test]
    fn partial_config_fills_remaining_fields() {
        let (config, error) = parse("refresh_interval_ms = 500");

        assert!(error.is_none());
        assert_eq!(config.refresh_interval_ms, 500);
        assert_eq!(config.switch, Config::default().switch);
    }

    #[test]
    fn malformed_config_falls_back_to_defaults() {
        let (config, error) = parse("refresh_interval_ms = [bad");

        assert_eq!(
            config.refresh_interval_ms,
            Config::default().refresh_interval_ms
        );
        assert!(matches!(error, Some(ConfigError::Invalid { .. })));
    }

    /// A known key with the wrong type is a config error, not something serde
    /// quietly defaults away.
    #[test]
    fn wrongly_typed_field_falls_back_to_defaults() {
        let (config, error) = parse("refresh_interval_ms = \"soon\"");

        assert_eq!(
            config.refresh_interval_ms,
            Config::default().refresh_interval_ms
        );
        assert!(matches!(error, Some(ConfigError::Invalid { .. })));
    }

    /// The line number is the actionable half of the message, so it has to
    /// point at the offending line and not at the start of the file.
    #[test]
    fn error_reports_the_offending_line() {
        let (_, error) = parse("switch = 'r'\nesc_quit = true\nrefresh_interval_ms = [bad");

        match error {
            Some(ConfigError::Invalid { line, .. }) => assert_eq!(line, Some(3)),
            other => panic!("expected an Invalid config error, got {other:?}"),
        }
    }

    /// The popup this lands in is sized before its text wraps, so a message
    /// that spills onto another line is a message the user cannot read.
    #[test]
    fn error_message_stays_on_one_short_line() {
        let (_, error) = parse("refresh_interval_ms = [bad");
        let error = error.expect("malformed config should report an error");
        let message = error.to_string();

        assert!(!message.contains('\n'), "message is multi-line: {message}");
        assert!(
            message.len() <= 60,
            "message is {} chars: {message}",
            message.len()
        );
        assert_eq!(error.path(), Some(Path::new("config.toml")));
    }

    #[test]
    fn error_line_is_absent_when_the_error_carries_no_span() {
        assert_eq!(
            error_line("switch = 'r'", &toml::de::Error::missing_field("x")),
            None
        );
    }
}
