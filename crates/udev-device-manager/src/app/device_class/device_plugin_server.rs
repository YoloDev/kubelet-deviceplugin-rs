use super::super::{DeviceHandle, DeviceTypeDistributor, DeviceTypeHandle};
use crate::{
  config::{DeviceClass, InternedString},
  utils::NotifySingle,
};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use futures::{FutureExt, Stream};
use kubelet_deviceplugin_proto::{tonic::Status, v1beta1};
use std::{
  pin::Pin,
  sync::Arc,
  task::{Context, Poll},
};

#[derive(Debug, Default)]
struct DevicesState {
  devices: Vec<DeviceHandle>,
  device_types: Vec<DeviceTypeHandle>,
}

#[derive(Debug)]
struct State {
  config: DeviceClass,
  devices: ArcSwap<DevicesState>,
  notifier: NotifySingle,
}

#[derive(Debug, Clone)]
pub struct DevicePlugin {
  state: Arc<State>,
}

impl DevicePlugin {
  pub fn new(config: DeviceClass) -> Self {
    Self {
      state: Arc::new(State {
        config,
        devices: ArcSwap::default(),
        notifier: NotifySingle::new(),
      }),
    }
  }

  fn config(&self) -> &DeviceClass {
    &self.state.config
  }

  pub fn subsystem(&self) -> InternedString {
    self.config().subsystem()
  }

  pub fn name(&self) -> InternedString {
    self.config().name()
  }

  pub fn reconcile(&self, distributor: &mut impl DeviceTypeDistributor) {
    let config = self.config();

    let device_types = distributor.get_device_types(|ty| config.match_with(ty).is_match());
    let devices = device_types.iter().flat_map(|ty| ty.devices()).collect();

    let devices = DevicesState {
      devices,
      device_types,
    };

    let new_state = Arc::new(devices);
    let old_state = self.state.devices.load();
    if old_state.devices != new_state.devices {
      drop(old_state);
      self.state.devices.store(new_state);
      self.state.notifier.notify();
    }
  }
}

#[async_trait]
impl v1beta1::DevicePlugin for DevicePlugin {
  type ListAndWatchStream = DevicePluginStream;

  async fn list_and_watch(&self) -> Result<Self::ListAndWatchStream, Status> {
    Ok(DevicePluginStream::new(self))
  }

  async fn allocate(
    &self,
    request: v1beta1::AllocateRequest,
  ) -> Result<v1beta1::AllocateResponse, Status> {
    todo!()
  }
}

pub struct DevicePluginStream {
  plugin: DevicePlugin,
  notifier: Option<NotifySingle>,
}

impl DevicePluginStream {
  fn new(plugin: &DevicePlugin) -> Self {
    Self {
      plugin: plugin.clone(),
      notifier: None,
    }
  }

  fn get_response(&self) -> Result<v1beta1::ListAndWatchResponse, Status> {
    let device_state = self.plugin.state.devices.load().clone();
    let devices = device_state
      .devices
      .iter()
      .map(v1beta1::Device::from)
      .collect();

    Ok(v1beta1::ListAndWatchResponse { devices })
  }
}

impl Stream for DevicePluginStream {
  type Item = Result<v1beta1::ListAndWatchResponse, Status>;

  fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    let this = self.get_mut();
    loop {
      match &mut this.notifier {
        None => {
          this.notifier = Some(this.plugin.state.notifier.clone());
          return Poll::Ready(Some(this.get_response()));
        }

        Some(n) => match n.poll_unpin(cx) {
          Poll::Pending => return Poll::Pending,
          Poll::Ready(()) => {
            this.notifier = None;
            continue;
          }
        },
      }
    }
  }
}
