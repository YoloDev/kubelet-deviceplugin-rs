use serde::{
  de::{self, Unexpected, Visitor},
  Deserialize, Serialize,
};
use std::{fmt, num::NonZeroU8};

const EXCLUSIVE: &str = "exclusive";
// const SHARED: &str = "shared";

/// Rules for how many pods can access a single devices at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceAccess {
  /// Exclusive access - one device per pod
  Exclusive,

  // /// Shared access - this device can be claimed by an unbound number of pods at once.
  // Shared,
  /// Shared access with an upper bound of pods that can access the pod at once.
  AtMost(NonZeroU8),
}

impl From<DeviceAccess> for u8 {
  fn from(access: DeviceAccess) -> Self {
    match access {
      DeviceAccess::Exclusive => 1,
      DeviceAccess::AtMost(n) => n.get(),
    }
  }
}

impl From<DeviceAccess> for usize {
  fn from(access: DeviceAccess) -> Self {
    match access {
      DeviceAccess::Exclusive => 1,
      DeviceAccess::AtMost(n) => n.get() as usize,
    }
  }
}

impl Default for DeviceAccess {
  #[inline]
  fn default() -> Self {
    DeviceAccess::Exclusive
  }
}

impl Serialize for DeviceAccess {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    match self {
      DeviceAccess::Exclusive => serializer.serialize_str(EXCLUSIVE),
      // DeviceAccess::Shared => serializer.serialize_str(SHARED),
      DeviceAccess::AtMost(v) => serializer.serialize_u8(v.get()),
    }
  }
}

struct DeviceAccessVisitor;
impl<'de> Visitor<'de> for DeviceAccessVisitor {
  type Value = DeviceAccess;

  fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
    // write!(
    //   f,
    //   "'{}', '{}', or a positive number between 1 and 255, both inclusive",
    //   EXCLUSIVE, SHARED
    // )
    write!(
      f,
      "'{}' or a positive number between 1 and 255, both inclusive",
      EXCLUSIVE
    )
  }

  fn visit_u8<E>(self, v: u8) -> Result<Self::Value, E>
  where
    E: de::Error,
  {
    match v {
      0 => Err(E::invalid_value(Unexpected::Unsigned(v as u64), &self)),
      1 => Ok(DeviceAccess::Exclusive),
      n => Ok(DeviceAccess::AtMost(unsafe { NonZeroU8::new_unchecked(n) })),
    }
  }

  fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
  where
    E: de::Error,
  {
    match v {
      EXCLUSIVE => Ok(DeviceAccess::Exclusive),
      // SHARED => Ok(DeviceAccess::Shared),
      _ => Err(E::invalid_value(Unexpected::Str(v), &self)),
    }
  }
}

impl<'de> Deserialize<'de> for DeviceAccess {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    deserializer.deserialize_any(DeviceAccessVisitor)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_test::{assert_tokens, Token};

  #[test]
  fn exclusive_serde() {
    assert_tokens(&DeviceAccess::Exclusive, &[Token::Str(EXCLUSIVE)]);
  }

  // #[test]
  // fn shared_serde() {
  //   assert_tokens(&DeviceAccess::Shared, &[Token::Str(SHARED)]);
  // }

  #[test]
  fn atmost_serde() {
    assert_tokens(
      &DeviceAccess::AtMost(NonZeroU8::new(1).unwrap()),
      &[Token::U8(1)],
    );

    assert_tokens(
      &DeviceAccess::AtMost(NonZeroU8::new(100).unwrap()),
      &[Token::U8(100)],
    );
  }
}
