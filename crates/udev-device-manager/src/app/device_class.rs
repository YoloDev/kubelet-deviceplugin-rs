mod device_plugin_server;

use self::device_plugin_server::DevicePlugin;
use crate::{
  app::DeviceTypeDistributor,
  config::{DeviceClass, InternedString},
  utils::AggregateErrorExt,
};
use color_eyre::{eyre::WrapErr, Result};
use futures::future::join_all;
use kubelet_deviceplugin_proto::{v1beta1, KubernetesDevicePluginServer};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct DeviceClassHandle {
  plugin: DevicePlugin,
  server: KubernetesDevicePluginServer,
}

impl DeviceClassHandle {
  async fn new(config: DeviceClass) -> Result<Self> {
    let plugin = DevicePlugin::new(config);
    let server = v1beta1::KubeletDevicePluginV1Beta1::new(plugin.clone())
      .start(format!("udev/{}/{}", plugin.subsystem(), plugin.name()))
      .await
      .wrap_err("Failed to start kubelet plugin server")?;

    Ok(Self { plugin, server })
  }

  pub fn reconcile(&self, distributor: &mut impl DeviceTypeDistributor) {
    self.plugin.reconcile(distributor)
  }
}

#[derive(Debug, Default)]
pub struct DeviceClassRegistry {
  device_classes: BTreeMap<InternedString, DeviceClassHandle>,
}

impl DeviceClassRegistry {
  pub async fn new(device_classes: &[DeviceClass]) -> Result<Self> {
    let mut handles = BTreeMap::new();
    for item in device_classes {
      let handle = DeviceClassHandle::new(item.clone()).await?;
      handles.insert(handle.plugin.name(), handle);
    }

    Ok(Self {
      device_classes: handles,
    })
  }

  pub async fn stop(self) -> Result<()> {
    let servers = self.device_classes.into_values().map(|h| h.server);
    let results = join_all(servers.map(|s| s.abort())).await;

    results.collect_errors()
  }

  pub fn reconcile(&self, distributor: &mut impl DeviceTypeDistributor) {
    for handle in self.device_classes.values() {
      handle.reconcile(distributor);
    }
  }
}
