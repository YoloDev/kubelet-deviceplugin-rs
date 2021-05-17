mod device_actor;

use crate::{
  config::{Config, Device, DeviceClass, InternedString},
  config_manager::ConfigEvent,
  signal_manager::Reload,
  system::{self, System},
  Actor, ChildrenTypeExt,
};
use anyhow::format_err;
use anyhow::Result;
use async_trait::async_trait;
use bastion::{
  children::Children, context::BastionContext, distributor::Distributor, msg, prelude::ChildrenRef,
  supervisor::SupervisorRef,
};
use device_actor::DeviceActor;
use futures::lock::Mutex;
use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};
use tracing::{event, span, Instrument, Level, Span};

use self::device_actor::DeviceActorCommand;

enum Message {
  ConfigUpdated(Arc<Config>),
  ReloadRequestReceived,
}

#[derive(Clone, Debug)]
struct Group<T> {
  child_ref: ChildrenRef,
  distributor: Distributor,
  config: Arc<Mutex<Arc<T>>>,
}

#[derive(Debug, Clone)]
pub(crate) struct Reconciler {
  devices_supervisor: SupervisorRef,
  device_class_supervisor: SupervisorRef,
}

impl Reconciler {
  pub fn new(devices_supervisor: SupervisorRef, device_class_supervisor: SupervisorRef) -> Self {
    Self {
      devices_supervisor,
      device_class_supervisor,
    }
  }

  fn create_device_actor(&self, config: Arc<Device>) -> Group<Device> {
    let name = config.name;
    let config = Arc::new(Mutex::new(config));
    let distributor = Distributor::named(&format!("device:{}:command", name));
    let child_ref =
      DeviceActor::new(config.clone(), distributor).register(Some(&self.devices_supervisor));

    Group {
      child_ref,
      distributor,
      config,
    }
  }

  async fn reconcile(
    &self,
    config: &Config,
    devices: &mut BTreeMap<InternedString, Group<Device>>,
    device_classes: &mut BTreeMap<InternedString, Group<DeviceClass>>,
  ) -> Result<()> {
    let mut seen = BTreeSet::new();
    for device in &config.devices {
      seen.insert(device.name);
      let group = devices
        .entry(device.name)
        .or_insert_with(|| self.create_device_actor(device.clone()));

      let mut lock = group.config.lock().await;
      *lock = device.clone();
      group
        .distributor
        .tell_everyone(DeviceActorCommand::UpdateConfig(device.clone()))?;
    }

    let extranious = devices
      .keys()
      .filter(|k| !seen.contains(*k))
      .copied()
      .collect::<Vec<_>>();
    for unseen_device_id in extranious {
      let group = devices.remove(&unseen_device_id).unwrap();
      group
        .child_ref
        .stop()
        .map_err(|()| format_err!("Could not stop child group"))?;
    }

    Ok(())
  }
}

#[async_trait]
impl Actor for Reconciler {
  const NAME: &'static str = "reconciler";

  fn create_span(&self, ctx: &bastion::context::BastionContext) -> tracing::Span {
    bastion_children_span!("reconciler", ctx)
  }

  fn configure(&self, children: Children) -> Children {
    children
      .with_distributor(system::reconciler::commands())
      .with_distributor(system::config::events())
      .with_distributor(system::signals::events())
  }

  async fn run(self, ctx: BastionContext) -> Result<()> {
    event!(target: "udev-device-manager", Level::DEBUG, "Starting reconciler");

    let mut devices = BTreeMap::new();
    let mut device_classes = BTreeMap::new();

    let mut config = System::get_config().await?;
    self
      .reconcile(&config, &mut devices, &mut device_classes)
      .await?;

    event!(target: "udev-device-manager", Level::DEBUG, "Reconciler ready");
    loop {
      let msg = msg! {
        ctx.recv().await.map_err(|()| format_err!("Failed to receive message"))?,
        msg: ConfigEvent => {
          match msg {
            ConfigEvent::ConfigUpdated(config) => Some(Message::ConfigUpdated(config))
          }
        };
        _msg: Reload => Some(Message::ReloadRequestReceived);
        _: _ => None;
      };

      if let Some(msg) = msg {
        match msg {
          Message::ConfigUpdated(c) => {
            event!(target: "udev-device-manager", Level::DEBUG, "Config updated - reconciling");
            config = c;
            self
              .reconcile(&config, &mut devices, &mut device_classes)
              .instrument(span!(target: "udev-device-manager", Level::INFO, "reconcile"))
              .await?;
          }

          Message::ReloadRequestReceived => {
            event!(target: "udev-device-manager", Level::DEBUG, "Reload requested - shutting down reconciler to restart the process");
            System::reload_config().await?;
            // this *should* tear down everything - and restart "from scratch"
            break;
          }
        }
      }
    }

    Ok(())
  }
}
