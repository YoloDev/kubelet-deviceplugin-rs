mod watcher;

use crate::{
  config::Config,
  system,
  utils::{BastionContextExt, BastionStreamExt},
  Actor, ConfigFormat,
};
use anyhow::{Error, Result};
use async_trait::async_trait;
use bastion::{children::Children, context::BastionContext, message::AnswerSender};
use futures::{future::ready, stream::select, StreamExt, TryStreamExt};
use notify::{DebouncedEvent, RecursiveMode};
use std::{path::PathBuf, result::Result as StdResult, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::{fs, io};
use tracing::{event, span, Level, Span};
use watcher::Watcher;

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

#[derive(Debug, Clone)]
pub enum ConfigEvent {
  ConfigUpdated(Arc<Config>),
}

#[derive(Debug, Clone)]
pub enum ConfigCommand {
  GetConfig,
  ForceReload,
}

#[derive(Debug, Clone)]
pub(crate) struct ConfigManager {
  file: PathBuf,
  format: ConfigFormat,
}

impl ConfigManager {
  pub fn new(file: PathBuf, format: ConfigFormat) -> Self {
    Self { file, format }
  }

  async fn read_config(&self) -> StdResult<Config, ConfigError> {
    let content = fs::read(&self.file).await?;

    let result = match self.format {
      ConfigFormat::Json => Ok(Json::parse_config(&content)?),
      ConfigFormat::Yaml => Ok(Yaml::parse_config(&content)?),
      ConfigFormat::Toml => Ok(Toml::parse_config(&content)?),
      ConfigFormat::Auto => match self.file.extension().and_then(|e| e.to_str()) {
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
}

#[derive(Debug)]
enum Message {
  GetConfig(AnswerSender),
  UpdateConfig(Option<AnswerSender>),
}

impl Message {
  fn from_command(command: ConfigCommand, sender: AnswerSender) -> Option<Message> {
    match command {
      ConfigCommand::GetConfig => Some(Message::GetConfig(sender)),
      ConfigCommand::ForceReload => Some(Message::UpdateConfig(Some(sender))),
    }
  }
}

#[async_trait]
impl Actor for ConfigManager {
  const NAME: &'static str = "config-manager";

  fn create_span(&self, ctx: &BastionContext) -> Span {
    bastion_children_span!("config-manager", ctx)
  }

  fn configure(&self, children: Children) -> Children {
    children.with_distributor(system::config::commands())
  }

  async fn run(self, ctx: BastionContext) -> Result<()> {
    let config = self.read_config().await?;
    let mut config = Arc::new(config);

    let mut watcher = Watcher::new(Duration::from_secs(2))?;
    watcher.watch(&self.file, RecursiveMode::NonRecursive)?;

    let file_events = watcher
      .filter_map(|e| match e {
        DebouncedEvent::Write(_) => ready(Some(Ok(Message::UpdateConfig(None)))),
        DebouncedEvent::Error(e, _) => ready(Some(Err(e))),
        _ => ready(None),
      })
      .map_err(Error::from);

    let bastion_messages = ctx
      .stream()
      .filter_map_bastion_message(|msg| msg.on_question(Message::from_command));

    let mut messages = select(file_events, bastion_messages);

    while let Some(message) = messages.try_next().await? {
      match message {
        Message::UpdateConfig(sender) => {
          event!(target: "udev-device-manager", Level::DEBUG, "Updating config");
          let new_config = self.read_config().await?;

          config = Arc::new(new_config);
          system::config::events().tell_everyone(ConfigEvent::ConfigUpdated(config.clone()))?;
          if let Some(sender) = sender {
            let _ = sender.reply(config.clone());
          }
        }

        Message::GetConfig(sender) => {
          let _ = sender.reply(config.clone());
        }
      }
    }

    Ok(())
  }
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

trait Format {
  fn parse_config(content: &[u8]) -> StdResult<Config, FormatError>;
}

struct Json;
impl Format for Json {
  fn parse_config(content: &[u8]) -> StdResult<Config, FormatError> {
    Ok(serde_json::from_slice(content)?)
  }
}

struct Yaml;
impl Format for Yaml {
  fn parse_config(content: &[u8]) -> StdResult<Config, FormatError> {
    Ok(serde_yaml::from_slice(content)?)
  }
}

struct Toml;
impl Format for Toml {
  fn parse_config(content: &[u8]) -> StdResult<Config, FormatError> {
    Ok(toml::from_slice(content)?)
  }
}
