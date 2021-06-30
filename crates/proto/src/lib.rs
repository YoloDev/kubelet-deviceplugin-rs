mod server;
pub(crate) mod transport;

#[cfg(feature = "v1beta1")]
pub mod v1beta1;

pub use server::KubernetesDevicePluginServer;
pub use tonic;
