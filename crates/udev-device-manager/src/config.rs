mod device;
mod device_class;
mod selector;
mod string;

use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub use device::{Device, DeviceAccess, UdevSelector};
pub use device_class::{DeviceClass, DeviceSelector};
pub use selector::MatchResult;
pub use string::InternedString;

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Config {
  pub(crate) devices: Vec<Arc<Device>>,

  pub(crate) device_classes: Vec<Arc<DeviceClass>>,
}
