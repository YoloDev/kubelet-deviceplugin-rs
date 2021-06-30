use anyhow::Error;
use bastion::{message::MessageHandler, prelude::SignedMessage};
use futures::{stream::FusedStream, Stream};
use pin_project::pin_project;
use std::{
  fmt,
  marker::PhantomData,
  pin::Pin,
  task::{Context, Poll},
};

#[pin_project]
#[must_use = "streams do nothing unless polled"]
pub struct FilterMapBasionMessage<St, F, T> {
  #[pin]
  stream: St,
  f: F,
  marker: PhantomData<fn() -> T>,
}

impl<St, F, T> FilterMapBasionMessage<St, F, T>
where
  St: Stream<Item = Result<SignedMessage, Error>>,
  F: (Fn(MessageHandler<Option<T>>) -> MessageHandler<Option<T>>) + Unpin,
{
  pub(super) fn new(stream: St, f: F) -> Self {
    Self {
      stream,
      f,
      marker: PhantomData,
    }
  }
}

impl<St, F, T> fmt::Debug for FilterMapBasionMessage<St, F, T>
where
  St: fmt::Debug,
{
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("FilterMapBasionMessage")
      .field("stream", &self.stream)
      .finish_non_exhaustive()
  }
}

impl<St, F, T> FusedStream for FilterMapBasionMessage<St, F, T>
where
  St: Stream<Item = Result<SignedMessage, Error>> + FusedStream,
  F: (Fn(MessageHandler<Option<T>>) -> MessageHandler<Option<T>>) + Unpin,
{
  fn is_terminated(&self) -> bool {
    self.stream.is_terminated()
  }
}

impl<St, F, T> Stream for FilterMapBasionMessage<St, F, T>
where
  St: Stream<Item = Result<SignedMessage, Error>>,
  F: (Fn(MessageHandler<Option<T>>) -> MessageHandler<Option<T>>) + Unpin,
{
  type Item = Result<T, Error>;

  fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    let mut this = self.project();

    loop {
      match this.stream.as_mut().poll_next(cx) {
        Poll::Pending => break Poll::Pending,
        Poll::Ready(None) => break Poll::Ready(None),
        Poll::Ready(Some(Err(e))) => break Poll::Ready(Some(Err(e))),
        Poll::Ready(Some(Ok(msg))) => {
          match (this.f)(MessageHandler::new(msg)).on_fallback(|_, _| None) {
            Some(v) => break Poll::Ready(Some(Ok(v))),
            None => continue,
          }
        }
      }
    }
  }
}
