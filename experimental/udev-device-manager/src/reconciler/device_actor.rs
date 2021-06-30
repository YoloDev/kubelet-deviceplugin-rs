use crate::{
  config::{Device, Labels, MatchResult},
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
use std::{
  collections::{btree_map::Entry, BTreeMap},
  sync::Arc,
};
use tracing::{event, Level, Span};

#[derive(Debug, Clone)]
pub enum DeviceActorCommand {
  UpdateConfig(Arc<Device>),
  GetInfo,
}

#[derive(Debug, Clone)]
pub enum DeviceActorEvent {
  InfoUpdated(DeviceTypeInfo),
  DevicesUpdated,
}

#[derive(Debug, Clone)]
pub struct DeviceTypeInfo {
  labels: Labels,
  events: Distributor,
}

#[derive(Debug, Clone)]
pub(crate) struct DeviceActor {
  config: Arc<Mutex<Arc<Device>>>,
  commands: Distributor,
  events: Distributor,
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
  pub fn new(config: Arc<Mutex<Arc<Device>>>, commands: Distributor, events: Distributor) -> Self {
    Self {
      config,
      commands,
      events,
    }
  }

  fn notify(&self, event: DeviceActorEvent) {
    let _ = system::device::events().tell_everyone(event.clone());
    let _ = self.events.tell_everyone(event);
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
      .with_distributor(self.commands)
      .with_distributor(system::device::commands())
      .with_distributor(system::udev::events())
  }

  async fn run(self, ctx: BastionContext) -> Result<()> {
    let mut config = self.config.lock().await.clone();
    let mut devices = System::get_udev_devices()
      .await?
      .into_iter()
      .filter(|dev| match_device(dev, &config))
      .map(|dev| (dev.syspath(), dev))
      .collect::<BTreeMap<_, _>>();

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
      let len = devices.len();
      match message {
        Message::UpdateConfig(c) => {
          event!(target: "udev-device-manager", Level::DEBUG, device.len = len, device.name = &*config.name, "received UpdateConfig");
          config = c;
          *self.config.lock().await = config.clone();
          // TODO: Re-scan devices
        }

        Message::GetInfo(sender) => {
          event!(target: "udev-device-manager", Level::DEBUG, device.len = len, device.name = &*config.name, "received GetInfo");
          let _ = sender.reply(DeviceTypeInfo {
            labels: config.labels.clone(),
            events: self.events,
          });
        }

        Message::DeviceUpserted(device) => match devices.entry(device.syspath()) {
          Entry::Vacant(entry) => {
            if match_device(&device, &config) {
              event!(target: "udev-device-manager", Level::DEBUG, device.len = len, device.name = &*config.name, "received DeviceUpserted on new matching device");
              entry.insert(device);
              self.notify(DeviceActorEvent::DevicesUpdated);
            }
          }
          Entry::Occupied(mut entry) => {
            event!(target: "udev-device-manager", Level::DEBUG, device.len = len, device.name = &*config.name, "received DeviceUpserted on existing device");
            if !match_device(&device, &config) {
              event!(target: "udev-device-manager", Level::DEBUG, device.len = len, device.name = &*config.name, "device no longer matches criteria - removed");
              entry.remove();
            } else {
              entry.insert(device);
            }
            self.notify(DeviceActorEvent::DevicesUpdated);
          }
        },

        Message::DeviceRemoved(device) => match devices.remove(&device.syspath()) {
          None => (),
          Some(_) => {
            event!(target: "udev-device-manager", Level::DEBUG, device.len = len, device.name = &*config.name, "received DeviceRemoved on existing device");
            self.notify(DeviceActorEvent::DevicesUpdated);
          }
        },
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

  match config
    .selector
    .match_with(&|name| device.attribute(name).and_then(|v| v.as_option()))
  {
    MatchResult::Matches => true,
    MatchResult::Mismatch(errors) => {
      event!(
        target: "udev-device-manager",
        Level::TRACE,
        device.subsystem = %device.subsystem(),
        device.syspath = %device.syspath(),
        device.devnode = %device.devnode(),
        device_type.name = &*config.name,
        "device does not match execpted attributes {:?}",
        errors);

      false
    }
  }
}
