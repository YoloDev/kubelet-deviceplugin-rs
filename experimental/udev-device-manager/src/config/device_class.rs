mod selector;

use super::InternedString;
use serde::{Deserialize, Serialize};

pub use selector::DeviceSelector;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceClass {
  /// Device class subsystem
  pub subsystem: InternedString,

  /// Device class name
  pub name: InternedString,

  /// Device class target
  pub target: InternedString,

  /// Selector to match against device groups
  pub selector: DeviceSelector,
}
