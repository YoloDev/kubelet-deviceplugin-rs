mod filter_map_bastion_message;

use anyhow::{format_err, Error};
use bastion::{context::BastionContext, message::MessageHandler, prelude::SignedMessage};
use futures::{future::BoxFuture, Stream};
use std::{
  future::Future,
  pin::Pin,
  task::{Context, Poll},
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
