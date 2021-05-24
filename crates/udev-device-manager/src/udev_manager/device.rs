use crate::config::InternedString;
use std::{collections::BTreeMap, convert::TryFrom, ffi::OsString, fmt, path::PathBuf, sync::Arc};
use thiserror::Error;

trait StrExt {
  fn intern(self) -> InternedString;
}

impl<'a> StrExt for &'a str {
  #[inline]
  fn intern(self) -> InternedString {
    InternedString::new(self)
  }
}

#[derive(Debug)]
struct Inner {
  subsystem: InternedString,
  syspath: InternedString,
  devnode: InternedString,
  attributes: BTreeMap<InternedString, InternedString>,
}

#[derive(Clone)]
pub struct Device(Arc<Inner>);

impl Device {
  pub fn subsystem(&self) -> InternedString {
    self.0.subsystem
  }

  pub fn syspath(&self) -> InternedString {
    self.0.syspath
  }

  pub fn devnode(&self) -> InternedString {
    self.0.devnode
  }

  pub fn attribute(&self, name: &str) -> Option<InternedString> {
    self.0.attributes.get(name).copied()
  }

  pub fn attributes(&self) -> &BTreeMap<InternedString, InternedString> {
    &self.0.attributes
  }
}

impl fmt::Debug for Device {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Debug::fmt(&*self.0, f)
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathKind {
  SysPath,
  DevNode,
}

#[derive(Debug, Error)]
pub enum DeviceError {
  #[error("Device has no subsystem")]
  NoSubsystem,

  #[error("Device has invalid subsystem: {subsystem:?}")]
  InvalidSubsystem { subsystem: OsString },

  #[error("Device path {path_kind:?} was not a valid string: {}", .value.display())]
  PathNotValidString { path_kind: PathKind, value: PathBuf },

  #[error("No devnode")]
  NoDevNode,

  #[error("Invalid attribute name: {name:?}")]
  InvalidAttributeName { name: OsString },

  #[error("Invalid attribute value for attribute '{name}': {value:?}")]
  InvalidAttributeValue {
    name: InternedString,
    value: OsString,
  },
}

impl DeviceError {
  fn invalid_path(path_kind: PathKind, value: impl Into<PathBuf>) -> Self {
    Self::PathNotValidString {
      path_kind,
      value: value.into(),
    }
  }

  fn invalid_attribute_name(name: impl Into<OsString>) -> Self {
    Self::InvalidAttributeName { name: name.into() }
  }

  fn invalid_attribute_value(name: InternedString, value: impl Into<OsString>) -> Self {
    Self::InvalidAttributeValue {
      name,
      value: value.into(),
    }
  }

  fn invalid_subsystem(subsystem: impl Into<OsString>) -> Self {
    Self::InvalidSubsystem {
      subsystem: subsystem.into(),
    }
  }
}

impl<'a> TryFrom<tokio_udev::Device> for Device {
  type Error = DeviceError;

  fn try_from(value: tokio_udev::Device) -> Result<Self, Self::Error> {
    let subsystem = value
      .subsystem()
      .ok_or(DeviceError::NoSubsystem)?
      .to_str()
      .ok_or_else(|| DeviceError::invalid_subsystem(value.subsystem().unwrap()))?
      .intern();
    let syspath = value
      .syspath()
      .to_str()
      .ok_or_else(|| DeviceError::invalid_path(PathKind::SysPath, value.syspath()))?
      .intern();
    let devnode = value
      .devnode()
      .ok_or(DeviceError::NoDevNode)?
      .to_str()
      .ok_or_else(|| DeviceError::invalid_path(PathKind::SysPath, value.syspath()))?
      .intern();

    let attributes = value
      .attributes()
      .map(|attribute| {
        let name = attribute
          .name()
          .to_str()
          .ok_or_else(|| DeviceError::invalid_attribute_name(attribute.name()))?
          .intern();

        let value = attribute
          .value()
          .to_str()
          .ok_or_else(|| DeviceError::invalid_attribute_value(name, attribute.name()))?
          .intern();

        Ok((name, value))
      })
      .collect::<Result<BTreeMap<_, _>, DeviceError>>()?;

    let inner = Inner {
      subsystem,
      syspath,
      devnode,
      attributes,
    };
    Ok(Device(Arc::new(inner)))
  }
}
