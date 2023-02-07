use std::{
    fmt, fs, io,
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    str::FromStr,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use url::Url;

macro_rules! apply_if_some {
    ($cfg:expr, $o:expr) => {
        if let Some(v) = $o {
            $cfg = v
        }
    };
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct AppConfig {
    pub url_base: Url,
    pub url_lobby_parameter: String,
    pub socket: u16,
    pub address: IpAddr,
    pub serve_from: PathBuf,
    pub private_key_file: PathBuf,
    pub certificate_chain_file: PathBuf,
    pub max_lobbies: usize,
    pub max_players: usize,
    #[serde(with = "as_secs")]
    pub heartbeat_interval: Duration,
    #[serde(with = "as_secs")]
    pub heartbeat_timeout: Duration,
}

pub struct AppConfigPartial {
    pub url_base: Option<Url>,
    pub url_lobby_parameter: Option<String>,
    pub socket: Option<u16>,
    pub address: Option<IpAddr>,
    pub serve_from: Option<PathBuf>,
    pub private_key_file: Option<PathBuf>,
    pub certificate_chain_file: Option<PathBuf>,
    pub max_lobbies: Option<usize>,
    pub max_players: Option<usize>,
    pub heartbeat_interval: Option<Duration>,
    pub heartbeat_timeout: Option<Duration>,
}

#[derive(Debug)]
pub enum AppConfigError {
    FailedToReadFile(io::Error),
    FailedToParseContents(toml::de::Error),
}

impl fmt::Display for AppConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FailedToReadFile(e) => write!(f, "failed to read file: {e}"),
            Self::FailedToParseContents(e) => write!(f, "failed to parse contents: {e}"),
        }
    }
}

impl std::error::Error for AppConfigError {}

impl AppConfig {
    pub fn from_file(path: &PathBuf) -> Result<Self, AppConfigError> {
        let cfg = fs::read_to_string(path).map_err(AppConfigError::FailedToReadFile)?;
        toml::from_str::<Self>(&cfg).map_err(AppConfigError::FailedToParseContents)
    }

    pub fn apply_partial(&mut self, cfg: AppConfigPartial) {
        apply_if_some!(self.url_base, cfg.url_base);
        apply_if_some!(self.url_lobby_parameter, cfg.url_lobby_parameter);
        apply_if_some!(self.socket, cfg.socket);
        apply_if_some!(self.address, cfg.address);
        apply_if_some!(self.serve_from, cfg.serve_from);
        apply_if_some!(self.private_key_file, cfg.private_key_file);
        apply_if_some!(self.certificate_chain_file, cfg.certificate_chain_file);
        apply_if_some!(self.max_lobbies, cfg.max_lobbies);
        apply_if_some!(self.max_players, cfg.max_players);
        apply_if_some!(self.heartbeat_interval, cfg.heartbeat_interval);
        apply_if_some!(self.heartbeat_timeout, cfg.heartbeat_timeout);
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            url_base: Url::from_str("https://localhost:8080").unwrap(),
            url_lobby_parameter: String::from("lobby"),
            socket: 8080,
            address: Ipv4Addr::new(127, 0, 0, 1).into(),
            serve_from: PathBuf::from_str("./static").unwrap(),
            private_key_file: PathBuf::from_str("./certs/key.pem").unwrap(),
            certificate_chain_file: PathBuf::from_str("./certs/cert.pem").unwrap(),
            max_lobbies: 100,
            max_players: 20,
            heartbeat_interval: Duration::from_secs(5),
            heartbeat_timeout: Duration::from_secs(30),
        }
    }
}

mod as_secs {
    use std::time::Duration;

    use serde::de::{Deserialize, Deserializer};
    use serde::ser::Serializer;

    pub fn serialize<S>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f64(value.as_secs_f64())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = f64::deserialize(deserializer)?;
        Ok(Duration::from_secs_f64(secs))
    }
}
