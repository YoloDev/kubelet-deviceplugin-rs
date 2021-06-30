use anyhow::Result;
use futures::Stream;
use notify::{DebouncedEvent, RecursiveMode, Watcher as WatcherTrait};
use pin_project::pin_project;
use std::{
  path::Path,
  pin::Pin,
  task::{Context, Poll},
  time::Duration,
};
use tokio::sync::mpsc::UnboundedReceiver;

#[pin_project]
pub struct Watcher {
  watcher: notify::RecommendedWatcher,

  #[pin]
  receiver: UnboundedReceiver<DebouncedEvent>,
}

impl Watcher {
  pub fn new(delay: Duration) -> Result<Self> {
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

  pub fn watch(&mut self, path: impl AsRef<Path>, recursive_mode: RecursiveMode) -> Result<()> {
    self.watcher.watch(path, recursive_mode).map_err(Into::into)
  }
}

impl Stream for Watcher {
  type Item = DebouncedEvent;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    self.receiver.poll_recv(cx)
  }
}
