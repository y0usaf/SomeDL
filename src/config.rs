use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub metadata: MetadataConfig,
    pub download: DownloadConfig,
    pub api: ApiConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MetadataConfig {
    #[serde(default = "default_true")]
    pub lyrics: bool,
    #[serde(default = "default_true")]
    pub copyright: bool,
    #[serde(default = "default_true")]
    pub isrc: bool,
    #[serde(default = "default_true")]
    pub genre: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DownloadConfig {
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_quality")]
    pub quality: u8,
    #[serde(default = "default_id3_version")]
    pub id3_version: u8,
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
    #[serde(default = "default_output")]
    pub output: String,
    #[serde(default)]
    pub cookies_path: String,
    #[serde(default)]
    pub cookies_from_browser: String,
    // runtime-only overrides (not in config file)
    #[serde(skip)]
    pub disable_download: bool,
    #[serde(skip)]
    pub strict_url_download: bool,
    #[serde(skip)]
    pub always_search_by_query: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiConfig {
    #[serde(default = "default_true")]
    pub musicbrainz: bool,
    #[serde(default = "default_true")]
    pub genius: bool,
    #[serde(default = "default_true")]
    pub genius_album_check: bool,
    #[serde(default)]
    pub genius_use_official: bool,
    #[serde(default)]
    pub genius_token: String,
    #[serde(default = "default_true")]
    pub deezer: bool,
    #[serde(default = "default_max_retry")]
    pub max_retry: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    #[serde(default = "default_level")]
    pub level: String,
    #[serde(default = "default_download_report")]
    pub download_report: usize,
    #[serde(default)]
    pub config_version: u32,
}

fn default_true() -> bool { true }
fn default_format() -> String { "mp3".to_string() }
fn default_quality() -> u8 { 5 }
fn default_id3_version() -> u8 { 3 }
fn default_output_dir() -> String { ".".to_string() }
fn default_output() -> String { "{artist} - {song}".to_string() }
fn default_max_retry() -> u32 { 3 }
fn default_level() -> String { "INFO".to_string() }
fn default_download_report() -> usize { 2 }

impl Default for Config {
    fn default() -> Self {
        Config {
            metadata: MetadataConfig {
                lyrics: true,
                copyright: true,
                isrc: true,
                genre: true,
            },
            download: DownloadConfig {
                format: "mp3".to_string(),
                quality: 5,
                id3_version: 3,
                output_dir: ".".to_string(),
                output: "{artist} - {song}".to_string(),
                cookies_path: String::new(),
                cookies_from_browser: String::new(),
                disable_download: false,
                strict_url_download: false,
                always_search_by_query: false,
            },
            api: ApiConfig {
                musicbrainz: true,
                genius: true,
                genius_album_check: true,
                genius_use_official: false,
                genius_token: String::new(),
                deezer: true,
                max_retry: 3,
            },
            logging: LoggingConfig {
                level: "INFO".to_string(),
                download_report: 2,
                config_version: CONFIG_VERSION,
            },
        }
    }
}

pub fn config_path() -> PathBuf {
    let base = if cfg!(target_os = "windows") {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from("."))
    } else if cfg!(target_os = "macos") {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library")
            .join("Application Support")
    } else {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from(".").join(".config"))
    };
    base.join("SomeDL-rs").join("somedl_config.toml")
}

pub fn load_config() -> Config {
    let path = config_path();
    if !path.exists() {
        return Config::default();
    }
    let content = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Config::default(),
    };
    match toml::from_str::<Config>(&content) {
        Ok(mut cfg) => {
            // Check and upgrade config version
            if cfg.logging.config_version < CONFIG_VERSION {
                // For now just update version; future migrations go here
                cfg.logging.config_version = CONFIG_VERSION;
                let _ = save_config(&cfg);
            }
            cfg
        }
        Err(e) => {
            eprintln!("Warning: Failed to parse config file: {e}. Using defaults.");
            Config::default()
        }
    }
}

pub fn save_config(config: &Config) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create config directory")?;
    }
    let content = toml::to_string_pretty(config).context("Failed to serialize config")?;
    fs::write(&path, content).context("Failed to write config file")?;
    Ok(())
}

pub fn generate_config() -> Result<()> {
    let path = config_path();
    if path.exists() {
        print!("WARNING - Config file already exists. Overwrite? [y/N] > ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
            println!("Writing config file canceled.");
            return Ok(());
        }
    }
    println!("Generating config at {}", path.display());
    save_config(&Config::default())
}
