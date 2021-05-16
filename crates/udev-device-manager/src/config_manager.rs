mod watcher;

use crate::{
  config::Config,
  utils::{merge_streams, BastionContextExt, Choice2},
  Actor, ConfigFormat,
};
use anyhow::{format_err, Error, Result};
use async_trait::async_trait;
use bastion::{
  children::Children,
  context::BastionContext,
  dispatcher::BroadcastTarget,
  distributor::Distributor,
  message::{AnswerSender, MessageHandler},
};
use futures::{future::ready, StreamExt, TryStreamExt};
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
pub(crate) struct ConfigManager {
  file: PathBuf,
  format: ConfigFormat,
  distributor: Distributor,
}

impl ConfigManager {
  pub fn new(file: PathBuf, format: ConfigFormat, distributor: Distributor) -> Self {
    Self {
      file,
      format,
      distributor,
    }
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
pub struct ConfigUpdated(Arc<Config>);
#[derive(Debug)]
pub struct GetConfig;

#[derive(Debug)]
enum Message {
  GetConfig(AnswerSender),
  UpdateConfig,
}

#[async_trait]
impl Actor for ConfigManager {
  const NAME: &'static str = "config-manager";

  fn create_span(&self, ctx: &BastionContext) -> Span {
    bastion_children_span!("config-manager", ctx)
  }

  fn configure(&self, children: Children) -> Children {
    children.with_distributor(self.distributor)
  }

  async fn run(self, ctx: BastionContext) -> Result<()> {
    let config = self.read_config().await?;
    let mut config = Arc::new(config);

    let mut watcher = Watcher::new(Duration::from_secs(2))?;
    watcher.watch(&self.file, RecursiveMode::NonRecursive)?;

    let file_events = watcher.filter_map(|e| match e {
      DebouncedEvent::Write(_) => ready(Some(Ok(()))),
      DebouncedEvent::Error(e, _) => ready(Some(Err(e))),
      _ => ready(None),
    });

    let messages = ctx.stream();
    let mut merged = merge_streams((file_events, messages)).filter_map(|choice| {
      ready(match choice {
        // Success paths
        Choice2::Choice1(Ok(())) => Some(Ok(Message::UpdateConfig)),
        Choice2::Choice2(Ok(msg)) => MessageHandler::new(msg)
          .on_question(|_: GetConfig, sender| Some(Ok(Message::GetConfig(sender))))
          .on_fallback(|_, _| None),

        // Error paths
        Choice2::Choice1(Err(e)) => Some(Err(Error::from(e))),
        Choice2::Choice2(Err(())) => Some(Err(format_err!("Failed to receive message"))),
      })
    });

    while let Some(msg) = merged.try_next().await? {
      match msg {
        Message::UpdateConfig => {
          let new_config = self.read_config().await?;

          config = Arc::new(new_config);
          ctx.broadcast_message(BroadcastTarget::All, ConfigUpdated(config.clone()));
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
