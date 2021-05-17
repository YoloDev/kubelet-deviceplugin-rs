use crate::config::{
  selector::{Selector, SelectorType},
  InternedString,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UdevSelector {
  #[serde(flatten)]
  selector: Selector<Self>,
}

impl UdevSelector {
  pub fn matches(&self, get_value: &impl Fn(&str) -> Option<InternedString>) -> bool {
    self.selector.matches(get_value)
  }
}

impl SelectorType for UdevSelector {
  const FLAT_KEYS_NAME: Option<&'static str> = Some("matchAttributes");
}
