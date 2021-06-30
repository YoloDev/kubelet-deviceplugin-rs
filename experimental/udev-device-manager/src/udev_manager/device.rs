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

trait UdevDeviceExt {
  fn hierarchy(&self) -> UdevHierarchy;
}

impl UdevDeviceExt for tokio_udev::Device {
  fn hierarchy(&self) -> UdevHierarchy {
    UdevHierarchy(Some(self.clone()))
  }
}

struct UdevHierarchy(Option<tokio_udev::Device>);

impl Iterator for UdevHierarchy {
  type Item = tokio_udev::Device;

  fn next(&mut self) -> Option<Self::Item> {
    match self.0.take() {
      None => None,
      Some(d) => {
        self.0 = d.parent();
        Some(d)
      }
    }
  }
}

#[derive(Clone, Copy)]
pub enum AttributeValue {
  None,
  Invalid,
  Value(InternedString),
}

impl fmt::Debug for AttributeValue {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      AttributeValue::None => f.write_str("None"),
      AttributeValue::Invalid => f.write_str("Invalid"),
      AttributeValue::Value(v) => fmt::Debug::fmt(v, f),
    }
  }
}

impl AttributeValue {
  pub fn as_option(self) -> Option<InternedString> {
    match self {
      Self::Value(v) => Some(v),
      _ => None,
    }
  }
}

#[derive(Debug)]
struct Inner {
  subsystem: InternedString,
  syspath: InternedString,
  devnode: InternedString,
  attributes: BTreeMap<InternedString, AttributeValue>,
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

  pub fn attribute(&self, name: &str) -> Option<AttributeValue> {
    self.0.attributes.get(name).copied()
  }

  pub fn attributes(&self) -> &BTreeMap<InternedString, AttributeValue> {
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
      .ok_or_else(|| DeviceError::invalid_path(PathKind::DevNode, value.syspath()))?
      .intern();

    let mut attributes = BTreeMap::new();
    for device in value.hierarchy() {
      for attribute in device.attributes() {
        let name = attribute
          .name()
          .to_str()
          .ok_or_else(|| DeviceError::invalid_attribute_name(attribute.name()))?
          .intern();

        if let Some(value) = device.attribute_value(attribute.name()) {
          let value = match value.to_str() {
            None => AttributeValue::Invalid,
            Some(v) if v.is_empty() => AttributeValue::None,
            Some(v) => AttributeValue::Value(v.intern()),
          };

          attributes.entry(name).or_insert(value);
        }
      }
    }

    let inner = Inner {
      subsystem,
      syspath,
      devnode,
      attributes,
    };
    Ok(Device(Arc::new(inner)))
  }
}
