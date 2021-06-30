mod access;
mod labels;
mod selector;

use super::InternedString;
use serde::{Deserialize, Serialize};

pub use access::DeviceAccess;
pub use labels::Labels;
pub use selector::UdevSelector;

/// A device is a combination of filters for selecting on udev
/// devices, and configuration for the matching devices. A single
/// physical device may end up in multiple "Device"-groups.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Device {
  /// Device group name - must be unique
  pub name: InternedString,

  /// Device subsystem
  pub subsystem: InternedString,

  /// Device access rules
  #[serde(default)]
  pub access: DeviceAccess,

  /// Device labels
  pub labels: Labels,

  /// Selector for filtering out udev devices
  pub selector: UdevSelector,
}
