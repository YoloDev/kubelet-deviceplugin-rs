use futures::{future::ready, Stream, StreamExt};
use signal_hook_tokio::Signals;
use std::{convert::TryFrom, fmt, io};
use thiserror::Error;
use tracing::{event, Level};

macro_rules! define_signals {
  (
    pub enum $name:ident {
      $($case:ident = $val:ident),+
      $(,)?
    }
  ) => {
    #[repr(i32)]
    pub enum $name {
      $($case = ::signal_hook::consts::$val,)+
    }

    impl TryFrom<i32> for $name {
      type Error = ();

      fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
          $(::signal_hook::consts::$val => Ok(Self::$case),)+
          _ => Err(()),
        }
      }
    }

    impl fmt::Debug for $name {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
          $(Self::$case => f.write_str(stringify!($val)),)+
        }
      }
    }

    impl fmt::Display for $name {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
          $(Self::$case => f.write_str(stringify!($val)),)+
        }
      }
    }

    impl $name {
      const ALL: &'static [i32] = &[$(::signal_hook::consts::$val,)+];
    }
  };
}

define_signals! {
  pub enum Signal {
    SigTerm = SIGTERM,
    SigInt = SIGINT,
    SigQuit = SIGQUIT,
    SigHup = SIGHUP,
  }
}

#[derive(Debug, Error)]
pub enum SignalWatchError {
  #[error(transparent)]
  Io(#[from] io::Error),
}

impl Signal {
  pub fn watch() -> Result<impl Stream<Item = Signal>, SignalWatchError> {
    let signals = Signals::new(Self::ALL)?;
    event!(target: "udev-device-manager", Level::DEBUG, "Started listening for termination signals");

    Ok(signals.filter_map(|s| ready(Signal::try_from(s).ok())))
  }
}
