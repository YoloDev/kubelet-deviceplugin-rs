use crate::{
  config::InternedString,
  udev::{UdevDevice, UdevEvent},
};
use color_eyre::Result;
use std::{collections::BTreeMap, convert::TryFrom};
use tokio_udev::Enumerator;
use tracing::{event, Level};

#[derive(Debug, Default)]
pub struct DeviceRegistry {
  devices: BTreeMap<InternedString, UdevDevice>,
}

impl DeviceRegistry {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn scan_devices(&mut self) -> Result<()> {
    event!(target: "udev-device-manager", Level::DEBUG, "gathering udev devices");
    let devices: BTreeMap<_, _> = Enumerator::new()?
      .scan_devices()?
      .filter_map(|d| UdevDevice::try_from(d).ok())
      .map(|d| (d.syspath(), d))
      .collect();
    event!(target: "udev-device-manager", Level::DEBUG, devices.len = devices.len(), "gathered {} udev devices", devices.len());

    self.devices = devices;
    Ok(())
  }

  pub fn update(&mut self, event: UdevEvent) {
    match event {
      UdevEvent::Add(device) | UdevEvent::Change(device) => {
        self.devices.insert(device.syspath(), device);
      }

      UdevEvent::Remove(device) => {
        self.devices.remove(&device.syspath());
      }

      UdevEvent::Bind(device) => {
        event!(target: "udev-device-manager", Level::DEBUG, device.syspath = %device.syspath(), device.devnode = %device.devnode(), "device bound");
      }

      UdevEvent::Unbind(device) => {
        event!(target: "udev-device-manager", Level::DEBUG, device.syspath = %device.syspath(), device.devnode = %device.devnode(), "device unbound");
      }

      UdevEvent::Unknown(device) => {
        event!(target: "udev-device-manager", Level::DEBUG, device.syspath = %device.syspath(), device.devnode = %device.devnode(), "unknown device event");
      }
    }
  }

  pub fn find<'a: 'f, 'f>(
    &'a self,
    mut f: impl FnMut(&UdevDevice) -> bool + 'f,
  ) -> impl Iterator<Item = UdevDevice> + 'f {
    self.devices.values().filter(move |d| f(*d)).cloned()
  }
}
