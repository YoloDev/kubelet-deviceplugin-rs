use crate::{
  config::{Device, MatchResult},
  system::{self, System},
  udev_manager::{Device as UdevDevice, UdevEvent},
  utils::{BastionContextExt, BastionStreamExt},
  Actor,
};
use anyhow::{Error, Result};
use async_trait::async_trait;
use bastion::{
  children::Children, context::BastionContext, distributor::Distributor, message::AnswerSender,
  prelude::RefAddr,
};
use futures::{executor::block_on, lock::Mutex, TryStreamExt};
use std::sync::Arc;
use tracing::{event, Level, Span};

#[derive(Debug, Clone)]
pub enum DeviceActorCommand {
  UpdateConfig(Arc<Device>),
  GetInfo,
}

#[derive(Debug, Clone)]
pub enum DeviceActorEvent {}

// #[derive(Debug, Default)]
// struct State {}

#[derive(Debug, Clone)]
pub(crate) struct DeviceActor {
  config: Arc<Mutex<Arc<Device>>>,
  // state: Arc<Mutex<State>>,
  distributor: Distributor,
}

enum Message {
  UpdateConfig(Arc<Device>),
  GetInfo(AnswerSender),
  DeviceUpserted(UdevDevice),
  DeviceRemoved(UdevDevice),
}

impl Message {
  fn from_command(command: DeviceActorCommand, sender: AnswerSender) -> Option<Message> {
    match command {
      DeviceActorCommand::UpdateConfig(config) => Some(Message::UpdateConfig(config)),
      DeviceActorCommand::GetInfo => Some(Message::GetInfo(sender)),
    }
  }

  fn from_udev_event(event: UdevEvent, _: RefAddr) -> Option<Message> {
    match event {
      UdevEvent::Upserted(device) => Some(Message::DeviceUpserted(device)),
      UdevEvent::Removed(device) => Some(Message::DeviceRemoved(device)),
    }
  }
}

impl DeviceActor {
  pub fn new(config: Arc<Mutex<Arc<Device>>>, distributor: Distributor) -> Self {
    Self {
      config,
      // state: Default::default(),
      distributor,
    }
  }

  fn notify(&self, event: DeviceActorEvent) {
    let _ = system::device::events().tell_everyone(event);
  }
}

#[async_trait]
impl Actor for DeviceActor {
  const NAME: &'static str = "device";

  fn create_span(&self, ctx: &BastionContext) -> Span {
    let name = block_on(async { self.config.lock().await.name });
    bastion_children_span!("device", ctx, device.name = &*name)
  }

  fn configure(&self, children: Children) -> Children {
    children
      .with_distributor(self.distributor)
      .with_distributor(system::device::commands())
      .with_distributor(system::udev::events())
  }

  async fn run(self, ctx: BastionContext) -> Result<()> {
    let mut config = self.config.lock().await.clone();
    let mut devices = System::get_udev_devices()
      .await?
      .into_iter()
      .filter(|dev| match_device(dev, &config))
      .collect::<Vec<_>>();

    event!(target: "udev-device-manager", Level::DEBUG, device.len = devices.len(), device.name = &*config.name, "device group has {} devices", devices.len());

    let bastion_messages = ctx
      .stream()
      .filter_map_bastion_message(|msg| {
        msg
          .on_question(Message::from_command)
          .on_tell(Message::from_udev_event)
      })
      .map_err(Error::from);

    let mut messages = bastion_messages;

    while let Some(message) = messages.try_next().await? {
      match message {
        Message::UpdateConfig(c) => {
          config = c;
          *self.config.lock().await = config.clone();
        }

        Message::GetInfo(_sender) => {
          todo!()
        }

        Message::DeviceUpserted(device) => {}

        Message::DeviceRemoved(device) => {}
      }
    }

    Ok(())
  }
}

fn match_device(device: &UdevDevice, config: &Device) -> bool {
  if device.subsystem() != config.subsystem {
    event!(
      target: "udev-device-manager",
      Level::TRACE,
      device.subsystem = %device.subsystem(),
      device.syspath = %device.syspath(),
      device.devnode = %device.devnode(),
      device_type.name = &*config.name,
      "device does not match expected subsystem {}",
      config.subsystem);
    return false;
  }

  match config.selector.match_with(&|name| device.attribute(name)) {
    MatchResult::Matches => true,
    MatchResult::Mismatch(errors) => {
      event!(
        target: "udev-device-manager",
        Level::TRACE,
        device.subsystem = %device.subsystem(),
        device.syspath = %device.syspath(),
        device.devnode = %device.devnode(),
        device_type.name = &*config.name,
        "device does not match execpted attributes {:#?}",
        errors);

      false
    }
  }
}
