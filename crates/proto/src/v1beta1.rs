mod proto;
mod types;

use async_trait::async_trait;
use futures::{stream::TryStream, Stream, TryStreamExt};
use hyper::{Server, Uri};
use std::{any::Any, convert::TryFrom, path::Path, pin::Pin, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::{io, net::UnixStream};
use tonic::transport::Endpoint;
use tower::service_fn;
use tracing::{span, Instrument, Level, Span};

pub use types::*;

use crate::transport::{Svc, UnixSocketListener};

/// Means that the device is healthy.
pub const HEALTHY: &str = "Healthy";

/// Means that the device is unhealthy.
pub const UNHEALTHY: &str = "Unhealthy";

/// Means current version of the API supported by kubelet.
pub const VERSION: &str = "v1beta1";

/// The folder the Device Plugin is expecting sockets to be on. Only
/// privileged pods have access to this path. Note: Placeholder until we
/// find a "standard path".
pub const DEVICE_PLUGIN_PATH: &str = "/var/lib/kubelet/device-plugins/";

/// The path of the kubelet registry socket.
pub const KUBELET_SOCKET: &str = "/var/lib/kubelet/device-plugins/kubelet.sock";

/// Timeout duration in secs for PreStartContainer RPC.
pub const KUBELET_PRE_START_CONTAINER_RPC_TIMEOUT_IN_SECS: Duration = Duration::from_secs(30);

#[async_trait]
pub trait ContainerPrestart: DevicePlugin {
  /// PreStartContainer is called, if indicated by Device Plugin during registeration phase,
  /// before each container start. Device plugin can run device specific operations
  /// such as resetting the device before making devices available to the container
  async fn prestart_container(
    &self,
    request: PreStartContainerRequest,
  ) -> Result<(), tonic::Status>;
}

#[async_trait]
pub trait PreferredAllocation: DevicePlugin {
  /// GetPreferredAllocation returns a preferred set of devices to allocate
  /// from a list of available ones. The resulting preferred allocation is not
  /// guaranteed to be the allocation ultimately performed by the
  /// devicemanager. It is only designed to help the devicemanager make a more
  /// informed allocation decision when possible.
  async fn get_preferred_allocation(
    &self,
    request: PreferredAllocationRequest,
  ) -> Result<PreferredAllocationResponse, tonic::Status>;
}

#[async_trait]
pub trait DevicePlugin: Send + Sync + 'static {
  type ListAndWatchStream: TryStream<Ok = ListAndWatchResponse, Error = tonic::Status>
    + Send
    + Sync
    + 'static;

  /// ListAndWatch returns a stream of List of Devices
  /// Whenever a Device state change or a Device disappears, ListAndWatch
  /// returns the new list
  async fn list_and_watch(&self) -> Result<Self::ListAndWatchStream, tonic::Status>;

  /// Allocate is called during container creation so that the Device
  /// Plugin can run device specific operations and instruct Kubelet
  /// of the steps to make the Device available in the container
  async fn allocate(&self, request: AllocateRequest) -> Result<AllocateResponse, tonic::Status>;
}

#[async_trait]
pub trait DevicePluginService: Send + Sync + 'static {
  type ListAndWatchStream: TryStream<Ok = ListAndWatchResponse, Error = tonic::Status>
    + Send
    + Sync
    + 'static;

  const PRE_START_REQUIRED: bool;
  const GET_PREFERRED_ALLOCATION_AVAILABLE: bool;

  /// ListAndWatch returns a stream of List of Devices
  /// Whenever a Device state change or a Device disappears, ListAndWatch
  /// returns the new list
  async fn list_and_watch(&self) -> Result<Self::ListAndWatchStream, tonic::Status>;

  /// Allocate is called during container creation so that the Device
  /// Plugin can run device specific operations and instruct Kubelet
  /// of the steps to make the Device available in the container
  async fn allocate(&self, request: AllocateRequest) -> Result<AllocateResponse, tonic::Status>;

  /// PreStartContainer is called, if indicated by Device Plugin during registeration phase,
  /// before each container start. Device plugin can run device specific operations
  /// such as resetting the device before making devices available to the container
  async fn prestart_container(
    &self,
    request: PreStartContainerRequest,
  ) -> Result<(), tonic::Status>;

  /// GetPreferredAllocation returns a preferred set of devices to allocate
  /// from a list of available ones. The resulting preferred allocation is not
  /// guaranteed to be the allocation ultimately performed by the
  /// devicemanager. It is only designed to help the devicemanager make a more
  /// informed allocation decision when possible.
  async fn get_preferred_allocation(
    &self,
    request: PreferredAllocationRequest,
  ) -> Result<PreferredAllocationResponse, tonic::Status>;
}

pub struct KubeletDevicePluginV1Beta1<
  T: DevicePlugin,
  const GET_PREFERRED_ALLOCATION_AVAILABLE: bool,
  const PRE_START_REQUIRED: bool,
>(Arc<T>);

impl<T: DevicePlugin> KubeletDevicePluginV1Beta1<T, false, false> {
  pub fn new(plugin: T) -> Self {
    Self(Arc::new(plugin))
  }
}

impl<T: DevicePlugin + PreferredAllocation, const GET_PREFERRED_ALLOCATION_AVAILABLE: bool>
  KubeletDevicePluginV1Beta1<T, false, GET_PREFERRED_ALLOCATION_AVAILABLE>
{
  pub fn with_preferred_allocation_support(
    self,
  ) -> KubeletDevicePluginV1Beta1<T, true, GET_PREFERRED_ALLOCATION_AVAILABLE> {
    KubeletDevicePluginV1Beta1(self.0)
  }
}

impl<T: DevicePlugin + ContainerPrestart, const PRE_START_REQUIRED: bool>
  KubeletDevicePluginV1Beta1<T, PRE_START_REQUIRED, false>
{
  pub fn with_prestart(self) -> KubeletDevicePluginV1Beta1<T, PRE_START_REQUIRED, true> {
    KubeletDevicePluginV1Beta1(self.0)
  }
}

#[async_trait]
impl<T: DevicePlugin> DevicePluginService for KubeletDevicePluginV1Beta1<T, false, false> {
  type ListAndWatchStream = T::ListAndWatchStream;

  const PRE_START_REQUIRED: bool = false;
  const GET_PREFERRED_ALLOCATION_AVAILABLE: bool = false;

  async fn list_and_watch(&self) -> Result<Self::ListAndWatchStream, tonic::Status> {
    self.0.list_and_watch().await
  }

  async fn allocate(&self, request: AllocateRequest) -> Result<AllocateResponse, tonic::Status> {
    self.0.allocate(request).await
  }

  async fn prestart_container(&self, _: PreStartContainerRequest) -> Result<(), tonic::Status> {
    Ok(())
  }

  async fn get_preferred_allocation(
    &self,
    _: PreferredAllocationRequest,
  ) -> Result<PreferredAllocationResponse, tonic::Status> {
    Err(tonic::Status::unimplemented(
      "get_preferred_allocation not supported",
    ))
  }
}

#[async_trait]
impl<T: DevicePlugin + PreferredAllocation> DevicePluginService
  for KubeletDevicePluginV1Beta1<T, true, false>
{
  type ListAndWatchStream = T::ListAndWatchStream;

  const PRE_START_REQUIRED: bool = false;
  const GET_PREFERRED_ALLOCATION_AVAILABLE: bool = true;

  async fn list_and_watch(&self) -> Result<Self::ListAndWatchStream, tonic::Status> {
    self.0.list_and_watch().await
  }

  async fn allocate(&self, request: AllocateRequest) -> Result<AllocateResponse, tonic::Status> {
    self.0.allocate(request).await
  }

  async fn prestart_container(&self, _: PreStartContainerRequest) -> Result<(), tonic::Status> {
    Ok(())
  }

  async fn get_preferred_allocation(
    &self,
    request: PreferredAllocationRequest,
  ) -> Result<PreferredAllocationResponse, tonic::Status> {
    self.0.get_preferred_allocation(request).await
  }
}

#[async_trait]
impl<T: DevicePlugin + ContainerPrestart> DevicePluginService
  for KubeletDevicePluginV1Beta1<T, false, true>
{
  type ListAndWatchStream = T::ListAndWatchStream;

  const PRE_START_REQUIRED: bool = true;
  const GET_PREFERRED_ALLOCATION_AVAILABLE: bool = false;

  async fn list_and_watch(&self) -> Result<Self::ListAndWatchStream, tonic::Status> {
    self.0.list_and_watch().await
  }

  async fn allocate(&self, request: AllocateRequest) -> Result<AllocateResponse, tonic::Status> {
    self.0.allocate(request).await
  }

  async fn prestart_container(
    &self,
    request: PreStartContainerRequest,
  ) -> Result<(), tonic::Status> {
    self.0.prestart_container(request).await
  }

  async fn get_preferred_allocation(
    &self,
    _: PreferredAllocationRequest,
  ) -> Result<PreferredAllocationResponse, tonic::Status> {
    Err(tonic::Status::unimplemented(
      "get_preferred_allocation not supported",
    ))
  }
}

#[async_trait]
impl<T: DevicePlugin + PreferredAllocation + ContainerPrestart> DevicePluginService
  for KubeletDevicePluginV1Beta1<T, true, true>
{
  type ListAndWatchStream = T::ListAndWatchStream;

  const PRE_START_REQUIRED: bool = true;
  const GET_PREFERRED_ALLOCATION_AVAILABLE: bool = true;

  async fn list_and_watch(&self) -> Result<Self::ListAndWatchStream, tonic::Status> {
    self.0.list_and_watch().await
  }

  async fn allocate(&self, request: AllocateRequest) -> Result<AllocateResponse, tonic::Status> {
    self.0.allocate(request).await
  }

  async fn prestart_container(
    &self,
    request: PreStartContainerRequest,
  ) -> Result<(), tonic::Status> {
    self.0.prestart_container(request).await
  }

  async fn get_preferred_allocation(
    &self,
    request: PreferredAllocationRequest,
  ) -> Result<PreferredAllocationResponse, tonic::Status> {
    self.0.get_preferred_allocation(request).await
  }
}

type ListAndWatchProtoStream =
  dyn Stream<Item = Result<proto::ListAndWatchResponse, tonic::Status>> + Send + Sync + 'static;

#[async_trait]
impl<T: DevicePluginService> proto::device_plugin_server::DevicePlugin for T {
  type ListAndWatchStream = Pin<Box<ListAndWatchProtoStream>>;

  async fn get_device_plugin_options(
    &self,
    _: tonic::Request<proto::Empty>,
  ) -> Result<tonic::Response<proto::DevicePluginOptions>, tonic::Status> {
    let options = proto::DevicePluginOptions {
      pre_start_required: Self::PRE_START_REQUIRED,
      get_preferred_allocation_available: Self::GET_PREFERRED_ALLOCATION_AVAILABLE,
    };

    Ok(tonic::Response::new(options))
  }

  async fn list_and_watch(
    &self,
    _: tonic::Request<proto::Empty>,
  ) -> Result<tonic::Response<Self::ListAndWatchStream>, tonic::Status> {
    let inner_stream = <Self as DevicePluginService>::list_and_watch(&self).await?;
    let mapped = inner_stream.map_ok(proto::ListAndWatchResponse::from);
    let boxed = Box::pin(mapped);

    Ok(tonic::Response::new(boxed))
  }

  async fn get_preferred_allocation(
    &self,
    request: tonic::Request<proto::PreferredAllocationRequest>,
  ) -> Result<tonic::Response<proto::PreferredAllocationResponse>, tonic::Status> {
    let response =
      <Self as DevicePluginService>::get_preferred_allocation(&self, request.into_inner().into())
        .await?;

    Ok(tonic::Response::new(
      proto::PreferredAllocationResponse::from(response),
    ))
  }

  async fn allocate(
    &self,
    request: tonic::Request<proto::AllocateRequest>,
  ) -> Result<tonic::Response<proto::AllocateResponse>, tonic::Status> {
    let response =
      <Self as DevicePluginService>::allocate(&self, request.into_inner().into()).await?;

    Ok(tonic::Response::new(proto::AllocateResponse::from(
      response,
    )))
  }

  async fn pre_start_container(
    &self,
    request: tonic::Request<proto::PreStartContainerRequest>,
  ) -> Result<tonic::Response<proto::PreStartContainerResponse>, tonic::Status> {
    <Self as DevicePluginService>::prestart_container(&self, request.into_inner().into()).await?;

    Ok(tonic::Response::new(proto::PreStartContainerResponse {}))
  }
}

impl<
    T: DevicePlugin,
    const GET_PREFERRED_ALLOCATION_AVAILABLE: bool,
    const PRE_START_REQUIRED: bool,
  > KubeletDevicePluginV1Beta1<T, GET_PREFERRED_ALLOCATION_AVAILABLE, PRE_START_REQUIRED>
where
  Self: proto::device_plugin_server::DevicePlugin,
{
  pub async fn start(
    self,
    resource_name: impl Into<String>,
  ) -> Result<Server<UnixSocketListener, impl Any>, ConnectionError> {
    let resource_name: String = resource_name.into();
    let span = span!(
      Level::INFO,
      "deviceplugin-v1beta1",
      resource = &*resource_name,
    );

    self._start(resource_name).instrument(span).await
  }

  async fn _start(
    self,
    resource_name: String,
  ) -> Result<Server<UnixSocketListener, impl Any>, ConnectionError> {
    let file_name = slug::slugify(&resource_name);
    let plugins_dir: &Path = DEVICE_PLUGIN_PATH.as_ref();
    let socket_path = {
      let mut index = 0usize;
      loop {
        let file_name = match index {
          0 => format!("{}.sock", file_name),
          v => format!("{}-{}.sock", file_name, v),
        };

        let path = plugins_dir.join(file_name);
        if !path.exists() {
          break path;
        }

        index += 1;
      }
    };

    let socket_listener = UnixSocketListener::bind(&socket_path)?;

    let device_plugin_service = proto::device_plugin_server::DevicePluginServer::new(self);
    let server = Server::builder(socket_listener)
      .http2_only(true)
      .serve(Svc::new(device_plugin_service, Some(Span::current())));
    // .http2_initial_connection_window_size(init_connection_window_size)
    // .http2_initial_stream_window_size(init_stream_window_size)
    // .http2_max_concurrent_streams(max_concurrent_streams)
    // .http2_keep_alive_interval(http2_keepalive_interval)
    // .http2_keep_alive_timeout(http2_keepalive_timeout)
    // .http2_max_frame_size(max_frame_size);

    let channel = Endpoint::try_from("http://[::]:50051")
      .unwrap()
      .connect_with_connector(service_fn(|_: Uri| {
        // Connect to a Uds socket
        UnixStream::connect(KUBELET_SOCKET)
      }))
      .await?;

    let mut kubelet_client = proto::registration_client::RegistrationClient::new(channel);
    kubelet_client
      .register(proto::RegisterRequest {
        version: VERSION.into(),
        endpoint: socket_path.to_string_lossy().into(),
        resource_name,
        options: Some(proto::DevicePluginOptions {
          pre_start_required: PRE_START_REQUIRED,
          get_preferred_allocation_available: GET_PREFERRED_ALLOCATION_AVAILABLE,
        }),
      })
      .await?;

    Ok(server)
  }
}

#[derive(Debug, Error)]
pub enum ConnectionError {
  #[error(transparent)]
  Transport(#[from] tonic::transport::Error),

  #[error(transparent)]
  Status(#[from] tonic::Status),

  #[error(transparent)]
  Io(#[from] io::Error),

  #[error(transparent)]
  Join(#[from] tokio::task::JoinError),
}
