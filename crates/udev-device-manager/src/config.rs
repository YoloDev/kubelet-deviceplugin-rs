mod device;
mod device_class;
mod selector;
mod string;

use serde::{Deserialize, Serialize};

pub use device::{Device, DeviceAccess, UdevSelector};
pub use device_class::{DeviceClass, DeviceSelector};
pub use string::InternedString;

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Config {
  devices: Vec<Device>,

  device_classes: Vec<DeviceClass>,
}
