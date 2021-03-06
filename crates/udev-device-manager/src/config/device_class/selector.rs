use crate::config::{
  selector::{Selector, SelectorType},
  InternedString, MatchResult,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DeviceTypeSelector {
  #[serde(flatten)]
  selector: Selector<Self>,
}

impl DeviceTypeSelector {
  pub fn match_with(&self, get_value: &impl Fn(&str) -> Option<InternedString>) -> MatchResult {
    self.selector.match_with(get_value)
  }
}

impl SelectorType for DeviceTypeSelector {
  const FLAT_KEYS_NAME: Option<&'static str> = Some("matchLabels");
}
