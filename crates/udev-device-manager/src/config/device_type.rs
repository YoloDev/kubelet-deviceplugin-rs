mod access;
mod labels;
mod selector;

use crate::udev::UdevDevice;

use super::{InternedString, MatchResult};
use serde::{Deserialize, Serialize};
use std::{fmt, sync::Arc};

pub use access::DeviceAccess;
pub use labels::DeviceTypeLabels;
pub use selector::UdevSelector;

mod inner {
  use super::*;

  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
  pub(super) struct DeviceType {
    /// Device group name - must be unique
    pub(super) name: InternedString,

    /// Device subsystem
    pub(super) subsystem: InternedString,

    /// Device access rules
    #[serde(default)]
    pub(super) access: DeviceAccess,

    /// Device labels
    pub(super) labels: DeviceTypeLabels,

    /// Selector for filtering out udev devices
    pub(super) selector: UdevSelector,
  }
}

#[derive(Clone, PartialEq)]
pub struct DeviceType {
  inner: Arc<inner::DeviceType>,
}

impl DeviceType {
  /// Device group name - must be unique
  pub fn name(&self) -> InternedString {
    self.inner.name
  }

  /// Device subsystem
  pub fn subsystem(&self) -> InternedString {
    self.inner.subsystem
  }

  /// Device access rules
  pub fn access(&self) -> DeviceAccess {
    self.inner.access
  }

  /// Device labels
  pub fn labels(&self) -> &DeviceTypeLabels {
    &self.inner.labels
  }

  /// Selector for filtering out udev devices
  pub fn selector(&self) -> &UdevSelector {
    &self.inner.selector
  }

  pub fn match_with(&self, device: &UdevDevice) -> MatchResult {
    let mut result = MatchResult::Matches;

    let subsystem = self.inner.subsystem;
    let device_subsystem = device.subsystem();
    if subsystem != device_subsystem {
      result += MatchResult::expected_value(
        InternedString::new_static("subsystem"),
        subsystem,
        Some(device_subsystem),
      );
    }

    result += self
      .selector()
      .match_with(&|name| device.attribute(name).and_then(|v| v.as_option()));

    result
  }
}

impl From<inner::DeviceType> for DeviceType {
  fn from(inner: inner::DeviceType) -> Self {
    Self {
      inner: Arc::new(inner),
    }
  }
}

impl fmt::Debug for DeviceType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Debug::fmt(&*self.inner, f)
  }
}

impl Serialize for DeviceType {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    Serialize::serialize(&*self.inner, serializer)
  }
}

impl<'de> Deserialize<'de> for DeviceType {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    <inner::DeviceType as Deserialize>::deserialize(deserializer).map(Self::from)
  }
}
