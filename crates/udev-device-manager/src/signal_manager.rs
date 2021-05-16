use anyhow::Result;
use async_trait::async_trait;
use bastion::{context::BastionContext, Bastion};
use futures::StreamExt;
use signal_hook::consts::signal::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
use signal_hook_tokio::Signals;
use tracing::{event, span, Level, Span};

use crate::Actor;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SignalManager;

#[async_trait]
impl Actor for SignalManager {
  const NAME: &'static str = "signal-manager";

  fn create_span(&self, ctx: &BastionContext) -> Span {
    bastion_children_span!("signal-manager", ctx)
  }

  async fn run(self, _: BastionContext) -> Result<()> {
    let mut signals = Signals::new(&[SIGHUP, SIGTERM, SIGINT, SIGQUIT])?;
    event!(target: "udev-device-manager", Level::DEBUG, "Started listening for termination signals");

    while let Some(signal) = signals.next().await {
      match signal {
        SIGHUP => {
          // TODO: trigger configuration reload
          event!(target: "udev-device-manager", Level::INFO, "Received SIGHUP, restarting");
          //ctx.broadcast_message(target, message)
        }
        signal => {
          let signal_name = match signal {
            SIGTERM => "SIGTERM",
            SIGINT => "SIGINT",
            SIGQUIT => "SIGQUIT",
            _ => unreachable!(),
          };
          event!(
            target: "udev-device-manager",
            Level::INFO,
            "Received signal {}, shutting down.",
            signal_name
          );

          Bastion::stop();
          break;
        }
      }
    }

    Ok(())
  }
}
