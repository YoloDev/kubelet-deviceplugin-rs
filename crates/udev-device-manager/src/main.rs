#![feature(trace_macros)]

use crate::{
  config_manager::ConfigManager, reconciler::Reconciler, signal_manager::SignalManager,
  system::System, udev_manager::UdevManager,
};
use anyhow::Result;
use async_trait::async_trait;
use bastion::{
  children::Children,
  context::BastionContext,
  prelude::ChildrenRef,
  supervisor::{
    ActorRestartStrategy, RestartPolicy, RestartStrategy, SupervisionStrategy, SupervisorRef,
  },
  Bastion,
};
use clap::Clap;
use futures::TryFutureExt;
use std::{path::PathBuf, time::Duration};
use tracing::{event, Instrument, Level, Span};
use tracing_subscriber::EnvFilter;

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
  ($name:expr, $ctx:expr $(, $($fields:tt)+)?) => {{
    let current = $ctx.current();
    ::tracing::span!(
      target: "udev-device-manager",
      ::tracing::Level::INFO,
      $name,
      ctx.current.name = current.name(),
      ctx.current.id = %current.id(),
      $($($fields)+)?)
  }};
}

mod config;
mod config_manager;
mod reconciler;
mod signal_manager;
mod system;
mod udev_manager;
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
  UdevManager.register(None);

  let config_supervisor = Bastion::supervisor(|supervisor| {
    supervisor.with_restart_strategy(RestartStrategy::new(
      RestartPolicy::Tries(3),
      ActorRestartStrategy::LinearBackOff {
        timeout: Duration::from_secs(1),
      },
    ))
  })
  .expect("failed to start config supervisor");
  ConfigManager::new(app.config_file, app.config_format).register(Some(&config_supervisor));

  let root_supervisor = Bastion::supervisor(|supervisor| {
    supervisor
      .with_strategy(SupervisionStrategy::OneForAll)
      .with_restart_strategy(RestartStrategy::new(
        RestartPolicy::Always,
        ActorRestartStrategy::Immediate,
      ))
  })
  .expect("could not create root supervisor group");

  let devices_supervisor = root_supervisor
    .supervisor(|supervisor| supervisor.with_strategy(SupervisionStrategy::OneForOne))
    .expect("failed to create devices supervisor");

  let device_class_supervisor = root_supervisor
    .supervisor(|supervisor| supervisor.with_strategy(SupervisionStrategy::OneForOne))
    .expect("failed to create device-class supervisor");

  Reconciler::new(devices_supervisor, device_class_supervisor).register(Some(&root_supervisor));

  event!(target: "udev-device-manager", Level::INFO, "Starting udev-device-manager");
  Bastion::start();

  // wait for config to be ready
  let result = System::get_config().await;

  if result.is_err() {
    event!(target: "udev-device-manager", Level::ERROR, "config manager seems to have died, shutting down the process: {:#?}", result);
    Bastion::stop();
  }

  Bastion::block_until_stopped();
  Ok(())
}
