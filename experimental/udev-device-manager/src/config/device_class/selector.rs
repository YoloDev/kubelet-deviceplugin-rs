use crate::config::selector::{Selector, SelectorType};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DeviceSelector {
  #[serde(flatten)]
  selector: Selector<Self>,
}

impl SelectorType for DeviceSelector {
  const FLAT_KEYS_NAME: Option<&'static str> = Some("matchLabels");
}
