mod args;
mod device_class;
mod device_registry;
mod device_type;

use self::{
  args::{Args, ConfigFormat},
  device_class::DeviceClassRegistry,
  device_registry::DeviceRegistry,
  device_type::{DeviceHandle, DeviceTypeDistributor, DeviceTypeHandle, DeviceTypeRegistry},
};
use crate::{
  app::args::LogFormat,
  config::{Config, ConfigError},
  signals::Signal,
  udev::{Udev, UdevDeviceError, UdevEvent},
};
use clap::Clap;
use color_eyre::{
  eyre::{eyre, Context},
  Result,
};
use futures::{pin_mut, select, StreamExt};
use std::{mem, path::PathBuf};
use tracing::{event, Level};
use tracing_subscriber::EnvFilter;

enum Action {
  None,
  Restart,
  Reconcile,
  Shutdown,
}

struct App {
  config_file: PathBuf,
  config_format: ConfigFormat,
  config: Config,
  devices: DeviceRegistry,
  device_types: DeviceTypeRegistry,
  device_classes: DeviceClassRegistry,
}

impl App {
  async fn new(config_file: PathBuf, config_format: ConfigFormat) -> Result<Self> {
    let config = Config::read(&config_file, config_format.into()).await?;

    let app = App {
      config_file,
      config_format,
      config,
      devices: DeviceRegistry::new(),
      device_types: DeviceTypeRegistry::default(),
      device_classes: DeviceClassRegistry::default(),
    };

    Ok(app)
  }

  async fn run(&mut self) -> Result<()> {
    let config_stream = Config::watch(self.config_file.clone(), self.config_format.into())?.fuse();
    pin_mut!(config_stream);

    let signal_stream = Signal::watch()?.fuse();
    pin_mut!(signal_stream);

    let udev_event_stream = Udev::watch().await?.fuse();
    pin_mut!(udev_event_stream);

    let mut action = Action::Restart;
    loop {
      action = match action {
        Action::Shutdown => break,
        Action::Restart => self.restart().await,
        Action::Reconcile => self.reconcile().await,
        Action::None => select! {
          c = config_stream.next() => self.on_config(c).await,
          s = signal_stream.next() => self.on_signal(s).await,
          e = udev_event_stream.next() => self.on_udev(e).await,
        },
      }?;
    }

    Ok(())
  }

  async fn restart(&mut self) -> Result<Action> {
    if let Err(e) = self.devices.scan_devices() {
      event!(
        target: "udev-device-manager",
        Level::ERROR,
        "Failed to read udev devices: {:#?}",
        e
      );
      return Err(e).context("app restart");
    }

    self.device_types = DeviceTypeRegistry::new(self.config.device_types());
    let device_classes = mem::replace(
      &mut self.device_classes,
      DeviceClassRegistry::new(self.config.device_classes()).await?,
    );
    device_classes.stop().await?;
    // TODO: Populate device classes

    Ok(Action::Reconcile)
  }

  async fn reconcile(&mut self) -> Result<Action> {
    self.device_types.reconcile(&self.devices);

    let mut distributor = self.device_types.distributor();
    self.device_classes.reconcile(&mut distributor);
    let remaining = distributor.remaining();
    event!(
      target: "udev-device-manager",
      Level::INFO,
      "{} device type remaining after distribution.",
      remaining.len(),
    );

    Ok(Action::None)
  }

  async fn on_config(&mut self, config: Option<Result<Config, ConfigError>>) -> Result<Action> {
    match config {
      None => {
        event!(
          target: "udev-device-manager",
          Level::ERROR,
          "Config watcher closed."
        );

        Err(eyre!("config watcher closed")).context("on_config")
      }

      Some(Err(e)) => {
        event!(
          target: "udev-device-manager",
          Level::ERROR,
          "Failed to read config: {:#?}",
          e
        );

        Err(e).context("on_config")
      }

      Some(Ok(c)) => {
        self.config = c;
        Ok(Action::Restart)
      }
    }
  }

  async fn on_signal(&mut self, signal: Option<Signal>) -> Result<Action> {
    match signal {
      None => {
        event!(
          target: "udev-device-manager",
          Level::ERROR,
          "Signal stream stopped, shutting down.",
        );

        Err(eyre!("signal stream stopped")).context("on_signal")
      }

      Some(Signal::SigHup) => {
        event!(target: "udev-device-manager", Level::INFO, "Received SIGHUP, restarting");
        Ok(Action::Restart)
      }

      Some(s) => {
        event!(
          target: "udev-device-manager",
          Level::INFO,
          "Received signal {}, shutting down.",
          s
        );
        Ok(Action::Shutdown)
      }
    }
  }

  async fn on_udev(&mut self, event: Option<Result<UdevEvent, UdevDeviceError>>) -> Result<Action> {
    match event {
      None => {
        event!(
          target: "udev-device-manager",
          Level::ERROR,
          "Udev stream stopped, shutting down.",
        );

        Err(eyre!("udev stream stopped")).context("on_udev")
      }

      Some(Err(e)) => {
        event!(
          target: "udev-device-manager",
          Level::ERROR,
          "Udev stream got an error, shutting down: {:#?}",
          e
        );

        Err(e).context("on_udev")
      }

      Some(Ok(e)) => {
        self.devices.update(e);
        Ok(Action::Reconcile)
      }
    }
  }
}

pub async fn run() -> Result<()> {
  let args = Args::parse();
  let filter = EnvFilter::from_default_env()
    // Set the base level when not matched by other directives to INFO.
    .add_directive(tracing::Level::INFO.into());

  match args.log_format {
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

  let mut app = App::new(args.config_file, args.config_format).await?;
  app.run().await?;

  Ok(())
}
