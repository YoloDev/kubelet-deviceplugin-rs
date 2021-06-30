use std::sync::Arc;

use crate::{
  config::Config,
  config_manager,
  udev_manager::{self, Device as UdevDevice},
};
use anyhow::Result;

pub enum System {}

macro_rules! define_group {
  ($ident:ident : group $name:ident) => {
    #[allow(unused)]
    pub(crate) fn $ident() -> Distributor {
      const NAME: &str = concat!(stringify!($name), ":commands");
      Distributor::named(NAME)
    }
  };

  ($ident:ident : notify $name:ident) => {
    #[allow(unused)]
    pub(crate) fn $ident() -> Distributor {
      const NAME: &str = concat!(stringify!($name), ":events");
      Distributor::named(NAME)
    }
  };

  ($ident:ident : $name:ident) => {
    pub(crate) mod $ident {
      use bastion::distributor::Distributor;

      define_group!(commands : group $name);
      define_group!(events : notify $name);
    }
  }
}

define_group!(config: config);
define_group!(reconciler: reconciler);
define_group!(udev: udev);
define_group!(signals: signals);
define_group!(device: device);

impl System {
  pub async fn get_config() -> Result<Arc<Config>> {
    config::commands()
      .request(config_manager::ConfigCommand::GetConfig)
      .await?
      .map_err(Into::into)
  }

  pub async fn reload_config() -> Result<Arc<Config>> {
    config::commands()
      .request(config_manager::ConfigCommand::ForceReload)
      .await?
      .map_err(Into::into)
  }

  pub async fn get_udev_devices() -> Result<Vec<UdevDevice>> {
    udev::commands()
      .request(udev_manager::UdevCommand::GetDevices)
      .await?
      .map_err(Into::into)
  }
}
