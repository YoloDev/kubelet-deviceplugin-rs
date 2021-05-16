#![feature(trace_macros)]

use anyhow::Result;
use async_trait::async_trait;
use bastion::{
  children::Children,
  context::BastionContext,
  distributor::Distributor,
  prelude::ChildrenRef,
  supervisor::{ActorRestartStrategy, RestartPolicy, RestartStrategy, SupervisorRef},
  Bastion, Callbacks,
};
use clap::Clap;
use futures::TryFutureExt;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tracing::{event, Instrument, Level, Span};
use tracing_subscriber::EnvFilter;

use crate::{
  config::Config, config_manager::ConfigManager, signal_manager::SignalManager,
  udev_events_manager::UdevEventsManager,
};

#[derive(Clap, Debug, PartialEq, Clone, Copy)]
enum LogFormat {
  Pretty,
  Json,
}

#[derive(Clap, Debug, PartialEq, Clone, Copy)]
enum ConfigFormat {
  Json,
  Yaml,
  Toml,
  Auto,
}

#[derive(Clap, Debug)]
struct App {
  /// Log output format
  #[clap(
    arg_enum,
    long = "log-format",
    short = 'f',
    env = "LOG_FORMAT",
    default_value = "pretty"
  )]
  log_format: LogFormat,

  /// Config file format
  #[clap(
    arg_enum,
    long = "config-format",
    short = 't',
    env = "CONFIG_FILE_FORMAT",
    default_value = "auto"
  )]
  config_format: ConfigFormat,

  /// Configuration file path
  #[clap(long = "config", short = 'c', env = "CONFIG_FILE")]
  config_file: PathBuf,
}

macro_rules! bastion_children_span {
  ($name:expr, $ctx:expr) => {{
    let current = $ctx.current();
    span!(target: "udev-device-manager", Level::INFO, $name, ctx.current.name = current.name(), ctx.current.id = %current.id())
  }};
}

mod config;
mod config_manager;
mod device_manager;
mod signal_manager;
mod udev_events_manager;
mod utils;

#[async_trait]
trait Actor: Sized + Send + Sync + Clone + 'static {
  const NAME: &'static str;

  fn create_span(&self, ctx: &BastionContext) -> Span;
  async fn run(self, ctx: BastionContext) -> Result<()>;

  fn configure(&self, children: Children) -> Children {
    children
  }
}

trait ChildrenTypeExt: Actor {
  fn init(self, children: Children) -> Children {
    self.configure(children).with_name(Self::NAME).with_exec(move |ctx| {
      let span = self.create_span(&ctx);

      self
        .clone()
        .run(ctx)
        .map_err(|error| {
          event!(target: "udev-device-manager", Level::ERROR, ?error, "Child '{}' returned an error", Self::NAME);
        })
        .instrument(span)
    })
  }

  fn register(self, supervisor: Option<&SupervisorRef>) -> ChildrenRef {
    if let Some(supervisor) = supervisor {
      supervisor
        .children(move |children| self.init(children))
        .unwrap_or_else(|_| panic!("failed to register {}", Self::NAME))
    } else {
      Bastion::children(move |children| self.init(children))
        .unwrap_or_else(|_| panic!("failed to register {}", Self::NAME))
    }
  }
}

impl<T: Actor> ChildrenTypeExt for T {}

#[tokio::main]
async fn main() -> Result<()> {
  let app = App::parse();
  let filter = EnvFilter::from_default_env()
    // Set the base level when not matched by other directives to INFO.
    .add_directive(tracing::Level::INFO.into());

  match app.log_format {
    LogFormat::Pretty => {
      tracing_subscriber::fmt().with_env_filter(filter).init();
    }
    LogFormat::Json => {
      tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .with_current_span(false)
        .with_span_list(false)
        .init();
    }
  }

  // let config_type = DispatcherType::Named("config".into());

  Bastion::init();
  SignalManager.register(None);
  UdevEventsManager.register(None);

  let config_distributor = Distributor::named("config");
  let config_supervisor = Bastion::supervisor(|supervisor| {
    supervisor.with_restart_strategy(RestartStrategy::new(
      RestartPolicy::Tries(3),
      ActorRestartStrategy::LinearBackOff {
        timeout: Duration::from_secs(1),
      },
    ))
  })
  .expect("failed to start config supervisor");
  ConfigManager::new(app.config_file, app.config_format, config_distributor)
    .register(Some(&config_supervisor));

  // let _signal_manager = Bastion::children(|children| {
  //   children.with_name("signals").with_exec(|_ctx| {
  //     async move {
  //       let mut signals = Signals::new(&[SIGHUP, SIGTERM, SIGINT, SIGQUIT])
  //         .expect("failed to register signal traps");

  //       while let Some(signal) = signals.next().await {
  //         match signal {
  //           SIGHUP => {
  //             // TODO: trigger configuration reload
  //             event!(Level::INFO, "Received SIGHUP, restarting");
  //             //ctx.broadcast_message(target, message)
  //           }
  //           signal => {
  //             let signal_name = match signal {
  //               SIGTERM => "SIGTERM",
  //               SIGINT => "SIGINT",
  //               SIGQUIT => "SIGQUIT",
  //               _ => unreachable!(),
  //             };
  //             event!(
  //               Level::INFO,
  //               "Received signal {}, shutting down.",
  //               signal_name
  //             );
  //             Bastion::stop();
  //             break;
  //           }
  //         }
  //       }

  //       Ok(())
  //     }
  //     .instrument(span!(Level::INFO, "signal-handler"))
  //   })
  // })
  // .expect("failed to setup signal manager");

  // let _udev_events_manager = Bastion::children(|children| {
  //   children
  //     .with_name("udev-events")
  //     .with_exec(|_ctx| async move {
  //       let socket = MonitorBuilder::new()?.listen()?;

  //       while let Some(event) = socket.next().await {
  //         match event?.event_type() {
  //           tokio_udev::EventType::Add => {}
  //           tokio_udev::EventType::Change => {}
  //           tokio_udev::EventType::Remove => {}
  //           tokio_udev::EventType::Bind => {}
  //           tokio_udev::EventType::Unbind => {}
  //           tokio_udev::EventType::Unknown => {}
  //         }
  //       }
  //     })
  // })
  // .expect("failed to setup udev events manager");

  // let _file_watcher = Bastion::children(|children| {
  //   children.with_name("config_watcher").with_exec(|_ctx| {
  //     async move {
  //       loop {
  //         tokio::time::sleep(Duration::from_secs(1)).await
  //       }
  //       Ok(())
  //     }
  //     .instrument(span!(Level::INFO, "config-watcher"))
  //   })
  // })
  // .expect("failed to start file watcher");

  // let supervisor = Bastion::supervisor(|supervisor| {
  //   let restart_strategy = RestartStrategy::default()
  //     .with_restart_policy(RestartPolicy::Tries(5))
  //     .with_actor_restart_strategy(ActorRestartStrategy::ExponentialBackOff {
  //       timeout: Duration::from_millis(5000),
  //       multiplier: 3.0,
  //     });

  //   supervisor.with_restart_strategy(restart_strategy)
  // })
  // .expect("failed to start supervisor");

  // let config = match read_config(&app.config_file).await {
  //   Ok(config) => config,
  //   Err(e) => {
  //     event!(Level::ERROR, path = %app.config_file.display(), "Failed to read configuration file. Error: {:?}", e);
  //     return Err(e);
  //   }
  // };

  // let signals = Signals::new(&[SIGHUP, SIGTERM, SIGINT, SIGQUIT])?;
  // let mut enumerator = Enumerator::new()?;
  // let devices = enumerator.scan_devices()?;
  // for device in devices.filter(|d| rule.matches(d)) {
  //   let devnode = device.devnode();
  //   let subsystem = device.subsystem();
  //   if let (Some(devnode), Some(subsystem)) = (devnode, subsystem) {
  //     let syspath = device.syspath();
  //     println!(
  //       " - {} ({:?} - {})",
  //       syspath.display(),
  //       subsystem,
  //       devnode.display()
  //     );
  //   }
  // }

  // Ok(())
  event!(target: "udev-device-manager", Level::INFO, "Starting udev-device-manager");
  Bastion::start();

  // wait for config to be ready
  let result: Result<Arc<Config>, _> = config_distributor
    .request(config_manager::GetConfig)
    .await?;

  if result.is_err() {
    event!(target: "udev-device-manager", Level::ERROR, "config manager seems to have died, shutting down the process");
    Bastion::stop();
  }

  Bastion::block_until_stopped();
  Ok(())
}
