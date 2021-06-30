use super::{Config, ConfigError, ConfigFormat};
use async_stream::stream;
use futures::{Stream, StreamExt};
use notify::{DebouncedEvent, RecursiveMode, Watcher as WatcherTrait};
use pin_project::pin_project;
use std::{
  path::Path,
  pin::Pin,
  task::{Context, Poll},
  time::Duration,
};
use thiserror::Error;
use tokio::{io, sync::mpsc::UnboundedReceiver};

#[pin_project]
struct Watcher {
  watcher: notify::RecommendedWatcher,

  #[pin]
  receiver: UnboundedReceiver<DebouncedEvent>,
}

impl Watcher {
  fn new(delay: Duration) -> Result<Self, ConfigWatcherError> {
    let (std_sender, std_receiver) = std::sync::mpsc::channel();
    let (async_sender, async_receiver) = tokio::sync::mpsc::unbounded_channel();
    let watcher = notify::watcher(std_sender, delay)?;
    std::thread::Builder::new()
      .name("file-watcher-mpsc".into())
      .spawn(move || {
        for evt in std_receiver {
          if async_sender.send(evt).is_err() {
            break;
          }
        }
      })?;

    Ok(Self {
      watcher,
      receiver: async_receiver,
    })
  }

  fn watch(
    &mut self,
    path: impl AsRef<Path>,
    recursive_mode: RecursiveMode,
  ) -> Result<(), ConfigWatcherError> {
    Ok(self.watcher.watch(path, recursive_mode)?)
  }
}

impl Stream for Watcher {
  type Item = DebouncedEvent;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    self.receiver.poll_recv(cx)
  }
}

#[derive(Debug, Error)]
pub enum ConfigWatcherError {
  #[error(transparent)]
  Notify(#[from] notify::Error),

  #[error(transparent)]
  Io(#[from] io::Error),
}

pub fn watch(
  file: impl AsRef<Path>,
  format: ConfigFormat,
) -> Result<impl Stream<Item = Result<Config, ConfigError>>, ConfigWatcherError> {
  let file = file.as_ref().to_owned();
  let mut watcher = Watcher::new(Duration::from_secs(30))?;
  watcher.watch(&file, RecursiveMode::NonRecursive)?;

  Ok(stream! {
    while let Some(event) = watcher.next().await {
      if let DebouncedEvent::Write(_) = event {
        yield Config::read(&file, format).await;
      }
    }
  })
}

// #[pin_project]
// pub struct ConfigWatcher {
//   #[pin]
//   watcher: Watcher,

//   current_read: Option<BoxFuture<'static, Result<Config, ConfigError>>>,

//   file: Arc<Path>,
//   format: ConfigFormat,
// }

// impl ConfigWatcher {
//   pub fn new(
//     file: impl AsRef<Path>,
//     format: ConfigFormat,
//   ) -> Result<ConfigWatcher, ConfigWatcherError> {
//     let file = Arc::from(file.as_ref());
//     let mut watcher = Watcher::new(Duration::from_secs(30))?;
//     watcher.watch(&file, RecursiveMode::NonRecursive)?;

//     Ok(ConfigWatcher {
//       watcher,
//       file,
//       format,
//       current_read: None,
//     })
//   }
// }

// impl Stream for ConfigWatcher {
//   type Item = Result<Config, ConfigError>;

//   fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
//     loop {
//       let this = self.as_mut().project();
//       if let Some(read) = this.current_read {
//         match read.poll_unpin(cx) {
//           Poll::Ready(config) => {
//             *this.current_read = None;
//             break Poll::Ready(Some(config));
//           }
//           Poll::Pending => break Poll::Pending,
//         }
//       }

//       match this.watcher.poll_next(cx) {
//         Poll::Ready(Some(DebouncedEvent::Write(_))) => {
//           *this.current_read = Some(Box::pin(Config::read(this.file.clone(), *this.format)));
//           continue;
//         }
//         Poll::Ready(None) => break Poll::Ready(None),
//         Poll::Ready(_) => continue,
//         Poll::Pending => break Poll::Pending,
//       }
//     }
//   }
// }
