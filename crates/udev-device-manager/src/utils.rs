mod filter_map_bastion_message;

use anyhow::Result;
use anyhow::{format_err, Error};
use bastion::{
  context::BastionContext,
  distributor::Distributor,
  message::MessageHandler,
  prelude::{SendError, SignedMessage},
};
use futures::{future::BoxFuture, Stream};
use pin_project::pin_project;
use std::{
  future::Future,
  pin::Pin,
  task::{Context, Poll},
  time::Duration,
};

pub use filter_map_bastion_message::FilterMapBasionMessage;

pub(crate) trait BastionStreamExt:
  Stream<Item = Result<SignedMessage, Error>> + Sized
{
  fn filter_map_bastion_message<F, T>(self, f: F) -> FilterMapBasionMessage<Self, F, T>
  where
    F: (Fn(MessageHandler<Option<T>>) -> MessageHandler<Option<T>>) + Unpin,
  {
    FilterMapBasionMessage::new(self, f)
  }
}

impl<'a> BastionStreamExt for BastionContextStream<'a> {}

pub(crate) trait BastionContextExt {
  type Stream: Stream<Item = Result<SignedMessage, Error>>;

  fn stream(self) -> Self::Stream;
}

impl<'a> BastionContextExt for &'a BastionContext {
  type Stream = BastionContextStream<'a>;

  fn stream(self) -> Self::Stream {
    BastionContextStream::new(self)
  }
}

pub(crate) struct BastionContextStream<'a> {
  ctx: &'a BastionContext,

  fut: BoxFuture<'a, Result<SignedMessage, ()>>,
}

impl<'a> BastionContextStream<'a> {
  fn new(ctx: &'a BastionContext) -> Self {
    let fut = Box::pin(ctx.recv());
    Self { ctx, fut }
  }
}

impl<'a> Stream for BastionContextStream<'a> {
  type Item = Result<SignedMessage, Error>;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    match Pin::new(&mut self.fut).poll(cx) {
      Poll::Pending => Poll::Pending,
      Poll::Ready(msg) => {
        let fut = Box::pin(self.ctx.recv());
        self.fut = fut;
        Poll::Ready(Some(
          msg.map_err(|()| format_err!("Failed to get bastion message")),
        ))
      }
    }
  }
}

pub(crate) trait DistributorExt<'a> {
  fn wait_for_responsive(self) -> WaitForResponsive<'a>;
}

impl<'a> DistributorExt<'a> for &'a Distributor {
  fn wait_for_responsive(self) -> WaitForResponsive<'a> {
    WaitForResponsive {
      distributor: self,
      delay: None,
    }
  }
}

#[pin_project]
#[derive(Debug)]
pub(crate) struct WaitForResponsive<'a> {
  distributor: &'a Distributor,

  #[pin]
  delay: Option<tokio::time::Sleep>,
}

impl<'a> Future for WaitForResponsive<'a> {
  type Output = Result<()>;

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    let this = self.project();
    let mut delay: Pin<&mut Option<tokio::time::Sleep>> = this.delay;
    let distributor: &'a Distributor = this.distributor;

    loop {
      match delay.as_mut().as_pin_mut() {
        None => match distributor.tell_one(()) {
          Ok(()) => break Poll::Ready(Ok(())),
          Err(SendError::EmptyRecipient) => {
            let sleep = tokio::time::sleep(Duration::from_millis(50));
            delay.set(Some(sleep));
          }
          Err(e) => break Poll::Ready(Err(e.into())),
        },
        Some(sleep) => match sleep.poll(cx) {
          Poll::Pending => break Poll::Pending,
          Poll::Ready(_) => {
            delay.set(None);
          }
        },
      }
    }
  }
}
