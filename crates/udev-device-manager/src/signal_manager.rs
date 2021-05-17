use crate::{system, Actor};
use anyhow::Result;
use async_trait::async_trait;
use bastion::{children::Children, context::BastionContext, Bastion};
use futures::StreamExt;
use signal_hook::consts::signal::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
use signal_hook_tokio::Signals;
use tracing::{event, span, Level, Span};

#[derive(Debug, Clone)]
pub struct Reload;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SignalManager;

#[async_trait]
impl Actor for SignalManager {
  const NAME: &'static str = "signal-manager";

  fn create_span(&self, ctx: &BastionContext) -> Span {
    bastion_children_span!("signal-manager", ctx)
  }

  fn configure(&self, children: Children) -> Children {
    children.with_distributor(system::signals::commands())
  }

  async fn run(self, _: BastionContext) -> Result<()> {
    let mut signals = Signals::new(&[SIGHUP, SIGTERM, SIGINT, SIGQUIT])?;
    event!(target: "udev-device-manager", Level::DEBUG, "Started listening for termination signals");

    while let Some(signal) = signals.next().await {
      match signal {
        SIGHUP => {
          event!(target: "udev-device-manager", Level::INFO, "Received SIGHUP, restarting");
          system::signals::events().tell_everyone(Reload)?;
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
