// most of this taken from tonic to be able to use hyper directly

use futures::{
  future::{ready, Ready},
  Stream,
};
use hyper::{server::accept::Accept, Body, Request, Response};
use std::{
  io::{self, IoSlice},
  path::Path,
  pin::Pin,
  task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::{body::BoxBody, codegen::Never, transport::server::Connected};
use tower::Service;
use tracing::{instrument::Instrumented, Instrument, Span};

pub struct UnixSocketListener(UnixListenerStream);
pub struct UnixSocket(tokio::net::UnixStream);

impl UnixSocketListener {
  pub fn bind<P>(path: P) -> io::Result<Self>
  where
    P: AsRef<Path>,
  {
    let listener = tokio::net::UnixListener::bind(path)?;
    Ok(Self(UnixListenerStream::new(listener)))
  }
}

impl Accept for UnixSocketListener {
  type Conn = UnixSocket;
  type Error = io::Error;

  fn poll_accept(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
    self.poll_next(cx)
  }
}

impl Stream for UnixSocketListener {
  type Item = io::Result<UnixSocket>;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    match Pin::new(&mut self.0).poll_next(cx) {
      Poll::Ready(None) => Poll::Ready(None),
      Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
      Poll::Ready(Some(Ok(s))) => Poll::Ready(Some(Ok(UnixSocket(s)))),
      Poll::Pending => Poll::Pending,
    }
  }
}

impl Connected for UnixSocket {}
impl AsyncRead for UnixSocket {
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<io::Result<()>> {
    Pin::new(&mut self.0).poll_read(cx, buf)
  }
}

impl AsyncWrite for UnixSocket {
  fn poll_write(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &[u8],
  ) -> Poll<Result<usize, io::Error>> {
    Pin::new(&mut self.0).poll_write(cx, buf)
  }

  fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
    Pin::new(&mut self.0).poll_flush(cx)
  }

  fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
    Pin::new(&mut self.0).poll_shutdown(cx)
  }

  fn poll_write_vectored(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    bufs: &[IoSlice<'_>],
  ) -> Poll<Result<usize, io::Error>> {
    Pin::new(&mut self.0).poll_write_vectored(cx, bufs)
  }

  fn is_write_vectored(&self) -> bool {
    self.0.is_write_vectored()
  }
}

// impl<T, B> Service<http::Request<B>> for DevicePluginServer<T>
//   where
//     T: DevicePlugin,
//     B: HttpBody + Send + Sync + 'static,
//     B::Error: Into<StdError> + Send + 'static,

// impl<S> Service<&UnixSocket> for S where S: Service<http::Request<Body>> {}
#[derive(Clone)]
pub(crate) struct Svc<S> {
  // concurrency_limit: Option<usize>,
  // timeout: Option<Duration>,
  inner: S,
  span: Option<Span>,
}

impl<S> Svc<S>
where
  S: Service<Request<Body>, Response = Response<BoxBody>, Error = Never>,
{
  pub fn new(service: S, span: Option<Span>) -> Self {
    Self {
      inner: service,
      span,
    }
  }
}

impl<S> Service<Request<Body>> for Svc<S>
where
  S: Service<Request<Body>, Response = Response<BoxBody>, Error = Never>,
{
  type Response = Response<BoxBody>;
  type Error = Never;
  type Future = Instrumented<S::Future>;

  fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    self.inner.poll_ready(cx)
  }

  fn call(&mut self, req: Request<Body>) -> Self::Future {
    let span = self.span.clone().unwrap_or_else(Span::none);
    self.inner.call(req).instrument(span)
  }
}

impl<'a, S> Service<&'a UnixSocket> for Svc<S>
where
  S: Service<Request<Body>, Response = Response<BoxBody>, Error = Never> + Clone,
{
  type Response = Self;
  type Error = Never;
  type Future = Ready<Result<Self::Response, Self::Error>>;

  fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    Poll::Ready(Ok(()))
  }

  fn call(&mut self, _: &'a UnixSocket) -> Self::Future {
    ready(Ok(self.clone()))
  }
}
