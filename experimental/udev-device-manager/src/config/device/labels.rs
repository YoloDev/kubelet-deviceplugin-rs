use crate::config::InternedString;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, sync::Arc};

#[derive(Clone, PartialEq)]
pub struct Labels {
  values: Arc<BTreeMap<InternedString, InternedString>>,
}

impl fmt::Debug for Labels {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.values.fmt(f)
  }
}

impl Serialize for Labels {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    (*self.values).serialize(serializer)
  }
}

impl<'de> Deserialize<'de> for Labels {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    <BTreeMap<InternedString, InternedString> as Deserialize<'de>>::deserialize(deserializer).map(
      |values| Labels {
        values: Arc::new(values),
      },
    )
  }
}
