use super::{UdevDevice, UdevDeviceError};
use crate::config::InternedString;
use futures::{Stream, StreamExt};
use pin_project::pin_project;
use std::{
  convert::{TryFrom, TryInto},
  io,
  pin::Pin,
  task::{Context, Poll},
};
use thiserror::Error;
use tokio::{
  runtime::Builder,
  select,
  sync::{
    mpsc::{channel, error::SendError, Receiver, Sender},
    oneshot::{self, error::RecvError},
  },
  task::{JoinError, LocalSet},
};
use tokio_udev::AsyncMonitorSocket;

#[derive(Clone, Debug)]
pub enum UdevEvent {
  Add(UdevDevice),
  Change(UdevDevice),
  Remove(UdevDevice),
  Bind(UdevDevice),
  Unbind(UdevDevice),
  Unknown(UdevDevice),
}

impl UdevEvent {
  pub fn device(&self) -> &UdevDevice {
    match self {
      UdevEvent::Add(device)
      | UdevEvent::Change(device)
      | UdevEvent::Remove(device)
      | UdevEvent::Bind(device)
      | UdevEvent::Unbind(device)
      | UdevEvent::Unknown(device) => device,
    }
  }

  pub fn event_type(&self) -> tokio_udev::EventType {
    match self {
      UdevEvent::Add(_) => tokio_udev::EventType::Add,
      UdevEvent::Change(_) => tokio_udev::EventType::Change,
      UdevEvent::Remove(_) => tokio_udev::EventType::Remove,
      UdevEvent::Bind(_) => tokio_udev::EventType::Bind,
      UdevEvent::Unbind(_) => tokio_udev::EventType::Unbind,
      UdevEvent::Unknown(_) => tokio_udev::EventType::Unknown,
    }
  }
}

impl TryFrom<tokio_udev::Event> for UdevEvent {
  type Error = <UdevDevice as TryFrom<tokio_udev::Device>>::Error;

  fn try_from(value: tokio_udev::Event) -> Result<Self, Self::Error> {
    let dev = value.device().try_into()?;
    Ok(match value.event_type() {
      tokio_udev::EventType::Add => Self::Add(dev),
      tokio_udev::EventType::Change => Self::Change(dev),
      tokio_udev::EventType::Remove => Self::Remove(dev),
      tokio_udev::EventType::Bind => Self::Bind(dev),
      tokio_udev::EventType::Unbind => Self::Unbind(dev),
      tokio_udev::EventType::Unknown => Self::Unknown(dev),
    })
  }
}

#[derive(Debug, Error)]
pub enum UdevBuilderError {
  #[error("Failed to send command to owning thread")]
  SendError,

  #[error("Failed to receive response from owning thread")]
  ReceiveError,

  #[error(transparent)]
  Io(#[from] io::Error),

  #[error(transparent)]
  Join(#[from] JoinError),
}

impl<T> From<SendError<T>> for UdevBuilderError {
  fn from(_: SendError<T>) -> Self {
    Self::SendError
  }
}

impl From<RecvError> for UdevBuilderError {
  fn from(_: RecvError) -> Self {
    Self::ReceiveError
  }
}

enum BuilderCommand {
  /// Adds a filter that matches events for devices with the given subsystem.
  MatchSubsystem(
    InternedString,
    oneshot::Sender<Result<(), UdevBuilderError>>,
  ),

  /// Adds a filter that matches events for devices with the given subsystem and device type.
  MatchSubsystemDevtype(
    InternedString,
    InternedString,
    oneshot::Sender<Result<(), UdevBuilderError>>,
  ),

  /// Adds a filter that matches events for devices with the given tag.
  MatchTag(
    InternedString,
    oneshot::Sender<Result<(), UdevBuilderError>>,
  ),

  /// Removes all filters currently set on the monitor.
  ClearFilters(oneshot::Sender<Result<(), UdevBuilderError>>),

  /// Listens for events matching the current filters.
  Listen(
    oneshot::Sender<
      Result<
        (
          Receiver<Result<UdevEvent, UdevDeviceError>>,
          oneshot::Sender<()>,
        ),
        UdevBuilderError,
      >,
    >,
  ),
}

pub struct UdevEventStreamBuilder {
  sender: Sender<BuilderCommand>,
}

impl UdevEventStreamBuilder {
  pub fn new() -> Result<Self, UdevBuilderError> {
    let (sender, receiver) = channel(1);
    std::thread::Builder::new()
      .name("udev-event-stream".into())
      .spawn(move || Self::bg_thread(receiver))?;

    Ok(Self { sender })
  }

  // /// Adds a filter that matches events for devices with the given subsystem.
  // pub async fn match_subsystem(self, subsystem: InternedString) -> Result<Self, UdevBuilderError> {
  //   let (sender, receiver) = oneshot::channel();
  //   self
  //     .sender
  //     .send(BuilderCommand::MatchSubsystem(subsystem, sender))
  //     .await?;
  //   receiver.await?;
  //   Ok(self)
  // }

  // /// Adds a filter that matches events for devices with the given subsystem and device type.
  // pub async fn match_subsystem_devtype(
  //   self,
  //   subsystem: InternedString,
  //   devtype: InternedString,
  // ) -> Result<Self, UdevBuilderError> {
  //   let (sender, receiver) = oneshot::channel();
  //   self
  //     .sender
  //     .send(BuilderCommand::MatchSubsystemDevtype(
  //       subsystem, devtype, sender,
  //     ))
  //     .await?;
  //   receiver.await?;
  //   Ok(self)
  // }

  // /// Adds a filter that matches events for devices with the given tag.
  // pub async fn match_tag(self, tag: InternedString) -> Result<Self, UdevBuilderError> {
  //   let (sender, receiver) = oneshot::channel();
  //   self
  //     .sender
  //     .send(BuilderCommand::MatchTag(tag, sender))
  //     .await?;
  //   receiver.await?;
  //   Ok(self)
  // }

  // /// Removes all filters currently set on the monitor.
  // pub async fn clear_filters(self) -> Result<Self, UdevBuilderError> {
  //   let (sender, receiver) = oneshot::channel();
  //   self
  //     .sender
  //     .send(BuilderCommand::ClearFilters(sender))
  //     .await?;
  //   receiver.await?;
  //   Ok(self)
  // }

  /// Listens for events matching the current filters.
  ///
  /// This method consumes the `Monitor`.
  pub async fn listen(self) -> Result<EventStream, UdevBuilderError> {
    let (sender, receiver) = oneshot::channel();
    self.sender.send(BuilderCommand::Listen(sender)).await?;
    let (receiver, signal) = receiver.await??;

    Ok(EventStream { receiver, signal })
  }

  fn bg_thread(receiver: Receiver<BuilderCommand>) -> Result<(), UdevBuilderError> {
    let rt = Builder::new_current_thread().enable_all().build()?;
    let local = LocalSet::new();
    let handle = local.spawn_local(Self::bg_task(receiver));
    rt.block_on(local);
    rt.block_on(handle)?
  }

  async fn bg_task(mut receiver: Receiver<BuilderCommand>) -> Result<(), UdevBuilderError> {
    let mut builder = tokio_udev::MonitorBuilder::new()?;
    let (socket, sender, signal_receiver) = loop {
      match receiver.recv().await {
        Some(v) => match v {
          BuilderCommand::MatchSubsystem(subsystem, ret) => {
            builder = match builder.match_subsystem(subsystem) {
              Ok(builder) => {
                ret.send(Ok(()));
                builder
              }
              Err(e) => {
                ret.send(Err(e.into()));
                return Ok(());
              }
            }
          }
          BuilderCommand::MatchSubsystemDevtype(subsystem, devtype, ret) => {
            builder = match builder.match_subsystem_devtype(subsystem, devtype) {
              Ok(builder) => {
                ret.send(Ok(()));
                builder
              }
              Err(e) => {
                ret.send(Err(e.into()));
                return Ok(());
              }
            }
          }
          BuilderCommand::MatchTag(tag, ret) => {
            builder = match builder.match_tag(tag) {
              Ok(builder) => {
                ret.send(Ok(()));
                builder
              }
              Err(e) => {
                ret.send(Err(e.into()));
                return Ok(());
              }
            }
          }
          BuilderCommand::ClearFilters(ret) => {
            builder = match builder.clear_filters() {
              Ok(builder) => {
                ret.send(Ok(()));
                builder
              }
              Err(e) => {
                ret.send(Err(e.into()));
                return Ok(());
              }
            }
          }
          BuilderCommand::Listen(ret) => match builder.listen().and_then(AsyncMonitorSocket::new) {
            Ok(socket) => {
              let (sender, receiver) = channel(1);
              let (signal_sender, signal_receiver) = oneshot::channel();
              ret.send(Ok((receiver, signal_sender)));
              break (socket, sender, signal_receiver);
            }
            Err(e) => {
              ret.send(Err(e.into()));
              return Ok(());
            }
          },
        },
        None => return Ok(()),
      }
    };

    let mut socket: AsyncMonitorSocket = socket;
    let mut signal = futures::stream::once(signal_receiver);
    loop {
      let e = select! {
        _ = signal.next() => return Ok(()),
        e = socket.next() => match e { None => return Ok(()), Some(e) => e },
      };

      let to_send = match e {
        Err(e) => Err(e.into()),
        Ok(evt) => match evt.try_into() {
          Ok(evt) => Ok(evt),
          Err(_) => continue,
        },
      };
      if let Err(e) = sender.send(to_send).await {
        return Ok(());
      }
    }
  }
}

#[pin_project]
pub struct EventStream {
  signal: oneshot::Sender<()>,

  #[pin]
  receiver: Receiver<Result<UdevEvent, UdevDeviceError>>,
}

impl Stream for EventStream {
  type Item = Result<UdevEvent, UdevDeviceError>;

  fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    self.project().receiver.poll_recv(cx)
  }
}
