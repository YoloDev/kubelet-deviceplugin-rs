use std::path::Path;

use super::Config;
use thiserror::Error;
use tokio::{fs, io};
use tracing::{event, Level};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ConfigFormat {
  Json,
  Yaml,
  Toml,
  Auto,
}

#[derive(Debug, Error)]
pub enum ConfigError {
  #[error("Invalid config file extension when using auto format: {0}")]
  InvalidExtension(String),

  #[error("Config file does not have a file extension, and format is set to auto")]
  MissingExtension,

  #[error("Failed to parse config file")]
  ParseError(#[from] FormatError),

  #[error(transparent)]
  Io(#[from] io::Error),
}

#[derive(Debug, Error)]
pub enum FormatError {
  #[error(transparent)]
  JsonError(#[from] serde_json::Error),

  #[error(transparent)]
  YamlError(#[from] serde_yaml::Error),

  #[error(transparent)]
  TomlError(#[from] toml::de::Error),
}

trait ConfigParser {
  fn parse_config(content: &[u8]) -> Result<Config, FormatError>;
}

struct Json;
impl ConfigParser for Json {
  fn parse_config(content: &[u8]) -> Result<Config, FormatError> {
    Ok(serde_json::from_slice(content)?)
  }
}

struct Yaml;
impl ConfigParser for Yaml {
  fn parse_config(content: &[u8]) -> Result<Config, FormatError> {
    Ok(serde_yaml::from_slice(content)?)
  }
}

struct Toml;
impl ConfigParser for Toml {
  fn parse_config(content: &[u8]) -> Result<Config, FormatError> {
    Ok(toml::from_slice(content)?)
  }
}

pub(super) async fn read_config(
  file: impl AsRef<Path>,
  format: ConfigFormat,
) -> Result<Config, ConfigError> {
  let file = file.as_ref();
  let content = fs::read(file).await?;

  let result = match format {
    ConfigFormat::Json => Ok(Json::parse_config(&content)?),
    ConfigFormat::Yaml => Ok(Yaml::parse_config(&content)?),
    ConfigFormat::Toml => Ok(Toml::parse_config(&content)?),
    ConfigFormat::Auto => match file.extension().and_then(|e| e.to_str()) {
      Some("toml") => Ok(Toml::parse_config(&content)?),
      Some("yaml") | Some("yml") => Ok(Yaml::parse_config(&content)?),
      Some("json") => Ok(Json::parse_config(&content)?),
      Some(other) => Err(ConfigError::InvalidExtension(other.into())),
      None => Err(ConfigError::MissingExtension),
    },
  };

  match result {
    Ok(config) => {
      event!(target: "udev-device-manager", Level::INFO, ?config, "Loaded configuration");
      Ok(config)
    }
    Err(error) => {
      event!(target: "udev-device-manager", Level::ERROR, ?error, "Failed to read config file");
      Err(error)
    }
  }
}
