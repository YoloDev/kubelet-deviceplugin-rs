use anyhow::Result;
use async_trait::async_trait;
use bastion::context::BastionContext;
use futures::TryStreamExt;
use tokio_udev::MonitorBuilder;
use tracing::{event, span, Level, Span};

use crate::Actor;

#[derive(Debug, Clone, Copy)]
pub(crate) struct UdevEventsManager;

#[async_trait]
impl Actor for UdevEventsManager {
  const NAME: &'static str = "udev-events-manager";

  fn create_span(&self, ctx: &BastionContext) -> Span {
    bastion_children_span!("udev-events-manager", ctx)
  }

  async fn run(self, _: BastionContext) -> Result<()> {
    let mut socket = MonitorBuilder::new()?.listen()?;

    while let Some(event) = socket.try_next().await? {
      event!(target: "udev-device-manager", Level::DEBUG, event.type_name = %event.event_type(), "udev event received");

      // TODO: Broadcast
      match event.event_type() {
        tokio_udev::EventType::Add => {}
        tokio_udev::EventType::Change => {}
        tokio_udev::EventType::Remove => {}
        tokio_udev::EventType::Bind => {}
        tokio_udev::EventType::Unbind => {}
        tokio_udev::EventType::Unknown => {}
      }
    }

    Ok(())
  }
}
