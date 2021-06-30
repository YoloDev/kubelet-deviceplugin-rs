use futures::{
  future::{Fuse, FusedFuture},
  FutureExt,
};
use pin_project::pin_project;
use static_assertions::assert_impl_all;
use std::{
  fmt,
  future::Future,
  panic,
  pin::Pin,
  task::{Context, Poll},
};
use tokio::{
  sync::oneshot::{self, Sender},
  task::JoinHandle,
};

#[pin_project]
pub struct Signal(#[pin] oneshot::Receiver<()>);

impl Future for Signal {
  type Output = ();

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    match self.project().0.poll(cx) {
      Poll::Pending => Poll::Pending,
      Poll::Ready(_) => Poll::Ready(()),
    }
  }
}

pub struct KubernetesDevicePluginServer {
  abort_channel: Sender<()>,
  handle: Fuse<JoinHandle<hyper::Result<()>>>,
}

assert_impl_all!(KubernetesDevicePluginServer: Unpin);

impl fmt::Debug for KubernetesDevicePluginServer {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct(stringify!(KubernetesDevicePluginServer))
      .finish_non_exhaustive()
  }
}

impl KubernetesDevicePluginServer {
  pub(crate) fn start(f: impl FnOnce(Signal) -> JoinHandle<hyper::Result<()>>) -> Self {
    let (abort_channel, receiver) = oneshot::channel::<()>();
    let handle = f(Signal(receiver)).fuse();

    Self {
      abort_channel,
      handle,
    }
  }

  pub async fn abort(self) -> hyper::Result<()> {
    if self.is_terminated() {
      return Ok(());
    }

    let _ = self.abort_channel.send(());

    match self.handle.await {
      Ok(result) => result,
      Err(e) if e.is_cancelled() => unreachable!(),
      Err(e) => panic::resume_unwind(e.into_panic()),
    }
  }

  pub fn is_terminated(&self) -> bool {
    self.handle.is_terminated()
  }
}

impl Future for KubernetesDevicePluginServer {
  type Output = hyper::Result<()>;

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    match self.get_mut().handle.poll_unpin(cx) {
      Poll::Pending => Poll::Pending,
      Poll::Ready(result) => match result {
        Ok(result) => Poll::Ready(result),
        Err(e) if e.is_cancelled() => unreachable!(),
        Err(e) => panic::resume_unwind(e.into_panic()),
      },
    }
  }
}

impl FusedFuture for KubernetesDevicePluginServer {
  fn is_terminated(&self) -> bool {
    self.is_terminated()
  }
}
