mod device;
mod event_stream;

use crate::{
  system,
  udev_manager::event_stream::EventStreamBuilder,
  utils::{BastionContextExt, BastionStreamExt},
  Actor,
};
use anyhow::{Error, Result};
use async_trait::async_trait;
use bastion::{children::Children, context::BastionContext, message::AnswerSender};
use futures::{future::ready, stream::select, StreamExt, TryStreamExt};
use std::{collections::BTreeMap, convert::TryFrom};
use tokio_udev::Enumerator;
use tracing::{event, span, Level, Span};

pub use device::Device;
pub use event_stream::DeviceEvent;

#[derive(Debug, Clone)]
pub enum UdevEvent {
  Upserted(Device),
  Removed(Device),
}

#[derive(Debug, Clone)]
pub enum UdevCommand {
  GetDevices,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct UdevManager;

enum Message {
  Udev(DeviceEvent),
  GetDevices(AnswerSender),
}

impl Message {
  fn from_command(command: UdevCommand, sender: AnswerSender) -> Option<Message> {
    match command {
      UdevCommand::GetDevices => Some(Message::GetDevices(sender)),
    }
  }
}

impl UdevManager {
  fn notify(&self, update: UdevEvent) {
    // this typically starts before any listeners, and we want to ignore not reaching anyone.
    // TODO: deal with different errors differently
    let _ = system::udev::events().tell_everyone(update);
  }

  fn notify_upserted(&self, device: Device) {
    self.notify(UdevEvent::Upserted(device))
  }

  fn notify_removed(&self, device: Device) {
    self.notify(UdevEvent::Removed(device))
  }
}

#[async_trait]
impl Actor for UdevManager {
  const NAME: &'static str = "udev-manager";

  fn create_span(&self, ctx: &BastionContext) -> Span {
    bastion_children_span!("udev-manager", ctx)
  }

  fn configure(&self, children: Children) -> Children {
    children.with_distributor(system::udev::commands())
  }

  async fn run(self, ctx: BastionContext) -> Result<()> {
    event!(target: "udev-device-manager", Level::DEBUG, "gathering udev devices");
    let mut devices: BTreeMap<_, _> = Enumerator::new()?
      .scan_devices()?
      .filter_map(|d| Device::try_from(d).ok())
      .map(|d| (d.syspath(), d))
      .collect();
    event!(target: "udev-device-manager", Level::DEBUG, devices.len = devices.len(), "gathered {} udev devices", devices.len());

    for device in devices.values().cloned() {
      event!(
        target: "udev-device-manager",
        Level::DEBUG,
        device.subsystem = %device.subsystem(),
        device.syspath = %device.syspath(),
        device.devnode = %device.devnode(),
        "disovered device");
      self.notify_upserted(device);
    }

    let udev_stream = EventStreamBuilder::new()?
      .listen()
      .await?
      .map(|e| e.map(Message::Udev));
    let bastion_stream = ctx
      .stream()
      .filter_map_bastion_message(|msg| msg.on_question(Message::from_command));

    let mut messages = select(udev_stream, bastion_stream);

    while let Some(message) = messages.try_next().await? {
      match message {
        Message::Udev(event) => {
          event!(
            target: "udev-device-manager",
            Level::DEBUG,
            event.type_name = %event.event_type(),
            event.device.syspath = %event.device().syspath(),
            event.device.devnode = %event.device().devnode(),
            "udev event received");

          match event {
            DeviceEvent::Add(device) | DeviceEvent::Change(device) => {
              devices.insert(device.syspath(), device.clone());
              self.notify_upserted(device);
            }

            DeviceEvent::Remove(device) => {
              devices.remove(&device.syspath());
              self.notify_removed(device);
            }

            DeviceEvent::Bind(_) | DeviceEvent::Unbind(_) | DeviceEvent::Unknown(_) => (),
          }
        }

        Message::GetDevices(sender) => {
          match sender.reply(devices.values().cloned().collect::<Vec<_>>()) {
            Ok(()) => (),
            Err(_) => {
              event!(target: "udev-device-manager", Level::DEBUG, "failed to send response on GetDevices")
            }
          }
        }
      }
    }

    Ok(())
  }
}
