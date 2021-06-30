use arc_swap::ArcSwap;
use color_eyre::{
  eyre::{self, eyre},
  Report, Section,
};
use std::{
  error::Error,
  fmt,
  future::Future,
  pin::Pin,
  sync::Arc,
  task::{Context, Poll, Waker},
};

pub trait AggregateErrorExt {
  fn collect_errors(self) -> Result<(), eyre::Error>;
}

enum AggregatorState<E>
where
  E: Error + Send + Sync + 'static,
{
  Single(E),
  Compound(Report),
}

impl<E> AggregatorState<E>
where
  E: Error + Send + Sync + 'static,
{
  fn add(self, error: E) -> Self {
    match self {
      AggregatorState::Single(e) => {
        AggregatorState::Compound(eyre!("encountered multiple errors").error(e).error(error))
      }
      AggregatorState::Compound(r) => AggregatorState::Compound(r.error(error)),
    }
  }

  fn into_report(self) -> Report {
    match self {
      AggregatorState::Single(e) => e.into(),
      AggregatorState::Compound(r) => r,
    }
  }
}

impl<I, E> AggregateErrorExt for I
where
  I: IntoIterator<Item = Result<(), E>>,
  E: Error + Send + Sync + 'static,
{
  fn collect_errors(self) -> Result<(), eyre::Error> {
    let mut iter = self.into_iter().filter_map(Result::err);
    match iter.next() {
      None => Ok(()),
      Some(e) => {
        let report = iter
          .fold(AggregatorState::Single(e), |s, e| s.add(e))
          .into_report();

        Err(report)
      }
    }
  }
}

struct NotifySingleState {
  waker: Option<Waker>,
  ready: bool,
}

impl Default for NotifySingleState {
  fn default() -> Self {
    Self {
      waker: None,
      ready: false,
    }
  }
}

#[derive(Clone, Default)]
pub struct NotifySingle {
  inner: Arc<ArcSwap<NotifySingleState>>,
}

impl fmt::Debug for NotifySingle {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct(stringify!(NotifySingle))
      .field("ready", &self.inner.load().ready)
      .finish_non_exhaustive()
  }
}

impl NotifySingle {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn notify(&self) {
    let state = self.inner.swap(Arc::new(NotifySingleState {
      waker: None,
      ready: true,
    }));

    let state = match Arc::try_unwrap(state) {
      Ok(v) => v,
      Err(_) => unreachable!(),
    };

    match state.waker {
      None => (),
      Some(waker) => waker.wake(),
    }
  }
}

impl Future for NotifySingle {
  type Output = ();

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    let state = self.inner.swap(Arc::new(NotifySingleState {
      waker: Some(cx.waker().clone()),
      ready: false,
    }));

    let state = match Arc::try_unwrap(state) {
      Ok(v) => v,
      Err(_) => unreachable!(),
    };

    match state.ready {
      true => Poll::Ready(()),
      false => Poll::Pending,
    }
  }
}
