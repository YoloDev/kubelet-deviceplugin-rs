use crate::{
  config::Device,
  system,
  utils::{BastionContextExt, BastionStreamExt},
  Actor,
};
use anyhow::{Error, Result};
use async_trait::async_trait;
use bastion::{
  children::Children, context::BastionContext, distributor::Distributor, message::AnswerSender,
};
use futures::{executor::block_on, lock::Mutex, TryStreamExt};
use std::sync::Arc;
use tracing::Span;

#[derive(Debug, Clone)]
pub enum DeviceActorCommand {
  UpdateConfig(Arc<Device>),
  GetInfo,
}

#[derive(Debug, Clone)]
pub enum DeviceActorEvent {}

#[derive(Debug, Default)]
struct State {}

#[derive(Debug, Clone)]
pub(crate) struct DeviceActor {
  config: Arc<Mutex<Arc<Device>>>,
  state: Arc<Mutex<State>>,
  distributor: Distributor,
}

enum Message {
  UpdateConfig(Arc<Device>),
  GetInfo(AnswerSender),
}

impl Message {
  fn from_command(command: DeviceActorCommand, sender: AnswerSender) -> Option<Message> {
    match command {
      DeviceActorCommand::UpdateConfig(config) => Some(Message::UpdateConfig(config)),
      DeviceActorCommand::GetInfo => Some(Message::GetInfo(sender)),
    }
  }
}

impl DeviceActor {
  pub fn new(config: Arc<Mutex<Arc<Device>>>, distributor: Distributor) -> Self {
    Self {
      config,
      state: Default::default(),
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
  }

  async fn run(self, ctx: BastionContext) -> Result<()> {
    let mut config = self.config.lock().await.clone();

    let bastion_messages = ctx
      .stream()
      .filter_map_bastion_message(|msg| msg.on_question(Message::from_command))
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
      }
    }

    Ok(())
  }
}
