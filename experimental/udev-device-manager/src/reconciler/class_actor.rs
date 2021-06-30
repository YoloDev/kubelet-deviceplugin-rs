use crate::config::DeviceClass;
use bastion::{distributor::Distributor, message::AnswerSender, prelude::RefAddr};
use std::sync::Arc;
use tokio::sync::Mutex;

use super::device_actor::DeviceActorEvent;

#[derive(Debug, Clone)]
pub enum DeviceClassActorCommand {
  UpdateConfig(Arc<DeviceClass>),
}

#[derive(Debug, Clone)]
pub enum DeviceClassActorEvent {}

#[derive(Debug, Clone)]
pub(crate) struct DeviceClassActor {
  config: Arc<Mutex<Arc<DeviceClass>>>,
  commands: Distributor,
  events: Distributor,
}

enum Message {
  UpdateConfig(Arc<DeviceClass>),
  DeviceTypeUpdated(DeviceTypeInfo, RefAddr),
  DevicesUpdated(RefAddr),
}

impl Message {
  fn from_command(command: DeviceClassActorCommand, sender: AnswerSender) -> Option<Message> {
    match command {
      DeviceClassActorCommand::UpdateConfig(config) => Some(Message::UpdateConfig(config)),
    }
  }

  // fn from_device_event(event: DeviceActorEvent, )
}
