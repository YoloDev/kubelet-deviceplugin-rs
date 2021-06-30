use crate::config::InternedString;
use arc_swap::RefCnt;
use std::{
  collections::BTreeMap, convert::TryFrom, ffi::OsString, fmt, io, path::PathBuf, sync::Arc,
};
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
pub struct Inner {
  id: InternedString,
  subsystem: InternedString,
  syspath: InternedString,
  devnode: InternedString,
  attributes: BTreeMap<InternedString, AttributeValue>,
}

#[derive(Clone)]
pub struct UdevDevice(Arc<Inner>);

impl UdevDevice {
  pub fn id(&self) -> InternedString {
    self.0.id
  }

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

impl fmt::Debug for UdevDevice {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Debug::fmt(&*self.0, f)
  }
}

// SAFETY: Just forwards all calls to the inner Arc
unsafe impl RefCnt for UdevDevice {
  type Base = Inner;

  #[inline]
  fn into_ptr(me: Self) -> *mut Self::Base {
    <Arc<Inner> as RefCnt>::into_ptr(me.0)
  }

  #[inline]
  fn as_ptr(me: &Self) -> *mut Self::Base {
    <Arc<Inner> as RefCnt>::as_ptr(&me.0)
  }

  #[inline]
  unsafe fn from_ptr(ptr: *const Self::Base) -> Self {
    Self(<Arc<Inner> as RefCnt>::from_ptr(ptr))
  }

  #[inline]
  fn inc(me: &Self) -> *mut Self::Base {
    <Arc<Inner> as RefCnt>::inc(&me.0)
  }

  #[inline]
  unsafe fn dec(ptr: *const Self::Base) {
    <Arc<Inner> as RefCnt>::dec(ptr)
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathKind {
  SysPath,
  DevNode,
}

#[derive(Debug, Error)]
pub enum UdevDeviceError {
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

  #[error(transparent)]
  Io(#[from] io::Error),
}

impl UdevDeviceError {
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

impl<'a> TryFrom<tokio_udev::Device> for UdevDevice {
  type Error = UdevDeviceError;

  fn try_from(value: tokio_udev::Device) -> Result<Self, Self::Error> {
    let subsystem = value
      .subsystem()
      .ok_or(UdevDeviceError::NoSubsystem)?
      .to_str()
      .ok_or_else(|| UdevDeviceError::invalid_subsystem(value.subsystem().unwrap()))?
      .intern();
    let syspath = value
      .syspath()
      .to_str()
      .ok_or_else(|| UdevDeviceError::invalid_path(PathKind::SysPath, value.syspath()))?
      .intern();
    let devnode = value
      .devnode()
      .ok_or(UdevDeviceError::NoDevNode)?
      .to_str()
      .ok_or_else(|| UdevDeviceError::invalid_path(PathKind::DevNode, value.syspath()))?
      .intern();

    let mut attributes = BTreeMap::new();
    for device in value.hierarchy() {
      for attribute in device.attributes() {
        let name = attribute
          .name()
          .to_str()
          .ok_or_else(|| UdevDeviceError::invalid_attribute_name(attribute.name()))?
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

    let id_hash = seahash::hash(syspath.as_bytes());
    let id_hash_bytes = id_hash.to_le_bytes();
    let id_string = base64::encode(&id_hash_bytes);
    let id = id_string.intern();
    let inner = Inner {
      id,
      subsystem,
      syspath,
      devnode,
      attributes,
    };
    Ok(UdevDevice(Arc::new(inner)))
  }
}
