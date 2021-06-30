mod device_class;
mod device_type;
mod parse;
mod selector;
mod string;
mod watch;

use futures::Stream;
use serde::{Deserialize, Serialize};
use std::{fmt, path::Path, sync::Arc};

pub use device_class::{DeviceClass, DeviceTypeSelector};
pub use device_type::{DeviceAccess, DeviceType, DeviceTypeLabels};
pub use parse::{ConfigError, ConfigFormat, FormatError};
pub use selector::{MatchResult, Mismatch};
pub use string::InternedString;
pub use watch::ConfigWatcherError;

mod inner {
  use super::*;

  #[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
  #[serde(rename_all = "camelCase")]
  pub(super) struct Config {
    #[serde(rename = "devices")]
    pub(super) device_typess: Vec<DeviceType>,

    pub(super) device_classes: Vec<DeviceClass>,
  }
}

#[derive(Clone, PartialEq)]
pub struct Config {
  inner: Arc<inner::Config>,
}

impl Config {
  /// Device types
  pub fn device_types(&self) -> &[DeviceType] {
    &self.inner.device_typess
  }

  /// Device classes (handlers)
  pub fn device_classes(&self) -> &[DeviceClass] {
    &self.inner.device_classes
  }
}

impl From<inner::Config> for Config {
  fn from(inner: inner::Config) -> Self {
    Self {
      inner: Arc::new(inner),
    }
  }
}

impl fmt::Debug for Config {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Debug::fmt(&*self.inner, f)
  }
}

impl Serialize for Config {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    Serialize::serialize(&*self.inner, serializer)
  }
}

impl<'de> Deserialize<'de> for Config {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    <inner::Config as Deserialize>::deserialize(deserializer).map(Self::from)
  }
}

impl Config {
  pub async fn read(file: impl AsRef<Path>, format: ConfigFormat) -> Result<Config, ConfigError> {
    parse::read_config(file, format).await
  }

  pub fn watch(
    file: impl AsRef<Path>,
    format: ConfigFormat,
  ) -> Result<impl Stream<Item = Result<Config, ConfigError>>, ConfigWatcherError> {
    watch::watch(file, format)
  }
}
