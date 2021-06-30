use super::DeviceRegistry;
use crate::{
  config::{DeviceType, InternedString},
  udev::UdevDevice,
};
use arc_swap::{ArcSwap, ArcSwapAny};
use kubelet_deviceplugin_proto::v1beta1;
use std::{collections::BTreeMap, sync::Arc};
use tracing::{event, Level};

#[derive(Debug)]
struct DeviceState {
  device: ArcSwapAny<UdevDevice>,
  id: InternedString,
}

#[derive(Debug, Clone)]
pub struct DeviceHandle(Arc<DeviceState>);

impl DeviceHandle {
  #[inline]
  fn state(&self) -> &DeviceState {
    &*self.0
  }

  pub fn new(device: UdevDevice, index: usize) -> Self {
    let id = InternedString::new(format!("{}:{}", device.id(), index));

    Self(Arc::new(DeviceState {
      device: ArcSwapAny::new(device),
      id,
    }))
  }

  pub fn update(&self, config: UdevDevice) {
    self.state().device.store(config)
  }

  pub fn config(&self) -> UdevDevice {
    self.state().device.load().clone()
  }

  pub fn id(&self) -> InternedString {
    self.state().id
  }
}

impl PartialEq for DeviceHandle {
  fn eq(&self, other: &Self) -> bool {
    // NOTE: we consider 2 devices to be the same if their ID is the same
    self.id() == other.id()
  }
}

impl<'a> From<&'a DeviceHandle> for v1beta1::Device {
  fn from(device: &'a DeviceHandle) -> Self {
    v1beta1::Device {
      id: device.id().into(),
      health: v1beta1::DeviceHealth::Healthy,
      topology: None,
    }
  }
}

#[derive(Debug)]
struct Inner {
  config: DeviceType,
  devices: ArcSwap<Vec<DeviceHandle>>,
}

#[derive(Debug, Clone)]
pub struct DeviceTypeHandle(Arc<Inner>);

impl DeviceTypeHandle {
  fn new(config: DeviceType) -> Self {
    Self(Arc::new(Inner {
      config,
      devices: ArcSwap::default(),
    }))
  }

  #[inline]
  fn inner(&self) -> &Inner {
    &*self.0
  }

  fn config(&self) -> &DeviceType {
    &self.inner().config
  }

  fn reconcile(&self, registry: &DeviceRegistry) {
    let config = self.config();
    let devices = registry
      .find(|d| config.match_with(d).is_match())
      .collect::<Vec<_>>();

    event!(
      target: "udev-device-manager",
      Level::DEBUG,
      device_type.name = %config.name(),
      device_type.subsystem = %config.subsystem(),
      device_type.devices.len = devices.len(),
      "device type matches {} devices",
      devices.len());

    let devices = devices
      .into_iter()
      .flat_map(|device| {
        (0usize..config.access().into()).map(move |index| DeviceHandle::new(device.clone(), index))
      })
      .collect::<Vec<_>>();
    let devices = Arc::new(devices);

    self.inner().devices.store(devices);
  }

  pub fn devices(&self) -> impl IntoIterator<Item = DeviceHandle> {
    self
      .inner()
      .devices
      .load()
      .clone()
      .iter()
      .cloned()
      .collect::<Vec<_>>()
  }
}

#[derive(Debug, Default)]
pub struct DeviceTypeRegistry {
  device_types: BTreeMap<InternedString, DeviceTypeHandle>,
}

impl DeviceTypeRegistry {
  pub fn new(devices: &[DeviceType]) -> Self {
    let device_types = devices
      .iter()
      .map(|d| (d.name(), DeviceTypeHandle::new(d.clone())))
      .collect();

    DeviceTypeRegistry { device_types }
  }

  pub fn reconcile(&self, registry: &DeviceRegistry) {
    for device in self.device_types.values() {
      device.reconcile(registry);
    }
  }

  pub(super) fn distributor<'a>(&'a mut self) -> Distributor<'a> {
    Distributor {
      types: self.device_types.values().collect(),
    }
  }
}

pub trait DeviceTypeDistributor {
  fn get_device_types(&mut self, f: impl FnMut(&DeviceType) -> bool) -> Vec<DeviceTypeHandle>;
}

pub(super) struct Distributor<'a> {
  types: Vec<&'a DeviceTypeHandle>,
}

impl<'a> Distributor<'a> {
  pub fn remaining(self) -> Vec<&'a DeviceTypeHandle> {
    self.types
  }
}

impl<'a> DeviceTypeDistributor for Distributor<'a> {
  fn get_device_types(&mut self, mut f: impl FnMut(&DeviceType) -> bool) -> Vec<DeviceTypeHandle> {
    let (hits, misses) = self.types.iter().partition(|d| f(d.config()));
    self.types = hits;
    misses.into_iter().map(|h| (*h).clone()).collect()
  }
}
