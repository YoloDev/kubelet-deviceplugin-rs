mod selector;

use super::{DeviceType, InternedString, MatchResult};
use serde::{Deserialize, Serialize};
use std::{fmt, sync::Arc};

pub use selector::DeviceTypeSelector;

mod inner {
  use super::*;

  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
  pub(super) struct DeviceClass {
    /// Device class subsystem
    pub subsystem: InternedString,

    /// Device class name
    pub name: InternedString,

    /// Device class target
    pub target: InternedString,

    /// Selector to match against device groups
    pub selector: DeviceTypeSelector,
  }
}

#[derive(Clone, PartialEq)]
pub struct DeviceClass {
  inner: Arc<inner::DeviceClass>,
}

impl DeviceClass {
  /// Device group name - must be unique
  pub fn name(&self) -> InternedString {
    self.inner.name
  }

  /// Device subsystem
  pub fn subsystem(&self) -> InternedString {
    self.inner.subsystem
  }

  /// Device class target
  pub fn target(&self) -> InternedString {
    self.inner.target
  }

  /// Selector for filtering out udev devices
  pub fn selector(&self) -> &DeviceTypeSelector {
    &self.inner.selector
  }

  pub fn match_with(&self, device_type: &DeviceType) -> MatchResult {
    let mut result = MatchResult::Matches;

    let subsystem = self.inner.subsystem;
    let device_subsystem = device_type.subsystem();
    if subsystem != device_subsystem {
      result += MatchResult::expected_value(
        InternedString::new_static("subsystem"),
        subsystem,
        Some(device_subsystem),
      );
    }

    let labels = device_type.labels();
    result += self.selector().match_with(&|name| labels.get(name));

    result
  }
}

impl From<inner::DeviceClass> for DeviceClass {
  fn from(inner: inner::DeviceClass) -> Self {
    Self {
      inner: Arc::new(inner),
    }
  }
}

impl fmt::Debug for DeviceClass {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Debug::fmt(&*self.inner, f)
  }
}

impl Serialize for DeviceClass {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    Serialize::serialize(&*self.inner, serializer)
  }
}

impl<'de> Deserialize<'de> for DeviceClass {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    <inner::DeviceClass as Deserialize>::deserialize(deserializer).map(Self::from)
  }
}
