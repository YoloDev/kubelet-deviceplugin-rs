use crate::config::selector::{Selector, SelectorType};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UdevSelector {
  #[serde(flatten)]
  selector: Selector<Self>,
}

impl SelectorType for UdevSelector {
  const FLAT_KEYS_NAME: Option<&'static str> = Some("matchAttributes");
}
