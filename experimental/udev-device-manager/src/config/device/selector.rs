use crate::config::{
  selector::{MatchResult, Selector, SelectorType},
  InternedString,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UdevSelector {
  #[serde(flatten)]
  selector: Selector<Self>,
}

impl UdevSelector {
  pub fn match_with(&self, get_value: &impl Fn(&str) -> Option<InternedString>) -> MatchResult {
    self.selector.match_with(get_value)
  }
}

impl SelectorType for UdevSelector {
  const FLAT_KEYS_NAME: Option<&'static str> = Some("matchAttributes");
}
