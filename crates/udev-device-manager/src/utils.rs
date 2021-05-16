use bastion::{context::BastionContext, prelude::SignedMessage};
use futures::{
  future::BoxFuture,
  stream::{Fuse, FusedStream},
  Stream, StreamExt,
};
use std::{
  future::Future,
  pin::Pin,
  task::{Context, Poll},
};

pub(crate) trait BastionContextExt {
  type Stream: Stream<Item = Result<SignedMessage, ()>>;

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
  type Item = Result<SignedMessage, ()>;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    match Pin::new(&mut self.fut).poll(cx) {
      Poll::Pending => Poll::Pending,
      Poll::Ready(msg) => {
        let fut = Box::pin(self.ctx.recv());
        self.fut = fut;
        Poll::Ready(Some(msg))
      }
    }
  }
}

pub(crate) trait MergeStreams {
  type Stream;

  fn merge(inputs: Self) -> Self::Stream;
}

#[inline]
pub(crate) fn merge_streams<T: MergeStreams>(inputs: T) -> <T as MergeStreams>::Stream {
  T::merge(inputs)
}

macro_rules! merge_streams {
  (@poll_all $choice:ident $cx:ident [] [$($all:ident,)+]) => {
    if $($all &&)+ true {
      return Poll::Ready(None);
    } else {
      return Poll::Pending;
    }
  };

  (@poll_all $choice:ident $cx:ident [$current:ident, $($remaining:ident,)*] [$($all:ident,)+]) => {
    let ($current, poll) = ($current.is_done(), Pin::new_unchecked($current).poll_next($cx));
    if let Poll::Ready(Some(item)) = poll {
      return Poll::Ready(Some($choice::$current(item)));
    } else {
      merge_streams!(@poll_all $choice $cx [$($remaining,)*] [$($all,)+]);
    }
  };

  (pub enum $choice:ident: $stream:ident {
    $($case:ident($t:ident)),+$(,)?
  }) => {
    pub enum $choice<$($t,)+> {
      $($case($t),)+
    }

    pub struct $stream<$($t,)+>(($(Fuse<$t>,)+)) where $($t: Stream,)+;

    impl<$($t,)+> MergeStreams for ($($t,)+)
    where $($t: Stream,)+ {
      type Stream = $stream<$($t,)+>;

      #[allow(non_snake_case)]
      fn merge(inputs: Self) -> Self::Stream {
        let ($($case,)+) = inputs;
        $stream(($($case.fuse(),)+))
      }
    }

    impl<$($t,)+> Stream for $stream<$($t,)+> where $($t: Stream,)+ {
      type Item = $choice<$(<$t as Stream>::Item,)+>;

      #[allow(non_snake_case)]
      fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
      ) -> Poll<Option<Self::Item>> {
        unsafe {
          let this = self.get_unchecked_mut();
          let ($($case,)+) = &mut this.0;
          merge_streams!(@poll_all $choice cx [$($case,)+] [$($case,)+]);
        }
      }
    }

    impl<$($t,)+> FusedStream for $stream<$($t,)+> where $($t: Stream,)+ {
      #[allow(non_snake_case)]
      fn is_terminated(&self) -> bool {
        let ($($case,)+) = &self.0;
        $($case.is_done() &&)+ true
      }
    }
  };
}

merge_streams! {
  pub enum Choice2: Choice2Stream {
    Choice1(T1),
    Choice2(T2),
  }
}

merge_streams! {
  pub enum Choice3: Choice3Stream {
    Choice1(T1),
    Choice2(T2),
    Choice3(T3),
  }
}
