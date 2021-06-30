use crate::config::InternedString;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, sync::Arc};

#[derive(Clone, PartialEq)]
pub struct DeviceTypeLabels {
  values: Arc<BTreeMap<InternedString, InternedString>>,
}

impl DeviceTypeLabels {
  pub fn get(&self, name: &str) -> Option<InternedString> {
    self.values.get(name).cloned()
  }
}

impl fmt::Debug for DeviceTypeLabels {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.values.fmt(f)
  }
}

impl Serialize for DeviceTypeLabels {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    (*self.values).serialize(serializer)
  }
}

impl<'de> Deserialize<'de> for DeviceTypeLabels {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    <BTreeMap<InternedString, InternedString> as Deserialize<'de>>::deserialize(deserializer).map(
      |values| DeviceTypeLabels {
        values: Arc::new(values),
      },
    )
  }
}
