mod device;

use crate::{
  system,
  utils::{BastionContextExt, BastionStreamExt},
  Actor,
};
use anyhow::{Error, Result};
use async_trait::async_trait;
use bastion::{children::Children, context::BastionContext, message::AnswerSender};
use device::Device;
use futures::{future::ready, stream::select, TryStreamExt};
use std::{collections::BTreeMap, convert::TryFrom};
use tokio_udev::{Enumerator, EventType, MonitorBuilder};
use tracing::{event, span, Level, Span};

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
  Udev(EventType, Device),
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
      self.notify_upserted(device);
    }

    let udev_stream = MonitorBuilder::new()?
      .listen()?
      .try_filter_map(|e| {
        ready(Ok(
          Device::try_from(e.device())
            .ok()
            .map(|d| Message::Udev(e.event_type(), d)),
        ))
      })
      .map_err(Error::from);
    let bastion_stream = ctx
      .stream()
      .filter_map_bastion_message(|msg| msg.on_question(Message::from_command));

    let mut messages = select(udev_stream, bastion_stream);

    while let Some(message) = messages.try_next().await? {
      match message {
        Message::Udev(event, device) => {
          event!(target: "udev-device-manager", Level::DEBUG, event.type_name = %event, event.device.syspath = %device.syspath(), "udev event received");

          match event {
            tokio_udev::EventType::Add | tokio_udev::EventType::Change => {
              devices.insert(device.syspath(), device.clone());
              self.notify_upserted(device);
            }

            tokio_udev::EventType::Remove => {
              devices.remove(&device.syspath());
              self.notify_removed(device);
            }

            tokio_udev::EventType::Bind
            | tokio_udev::EventType::Unbind
            | tokio_udev::EventType::Unknown => (),
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
