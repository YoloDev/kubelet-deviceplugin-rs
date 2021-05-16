use super::proto;
use std::collections::HashMap;

trait CustomInto<T> {
  fn custom_into(self) -> T;
}

// impl<T, U> CustomInto<T> for U
// where
//   U: Into<T>,
// {
//   #[inline]
//   fn custom_into(self) -> T {
//     self.into()
//   }
// }

macro_rules! reflective_into {
  ($($name:ty)+) => {
    $(
      impl CustomInto<$name> for $name {
        #[inline]
        fn custom_into(self) -> $name {
          self
        }
      }
    )*
  };
}

reflective_into!(bool i32 i64 String Vec<String> HashMap<String, String>);

macro_rules! custom_into {
  ($($name:ident)+) => {
    $(
      impl CustomInto<Option<$name>> for Option<proto::$name> {
        fn custom_into(self) -> Option<$name> {
          self.map(Into::into)
        }
      }

      impl CustomInto<Option<proto::$name>> for Option<$name> {
        fn custom_into(self) -> Option<proto::$name> {
          self.map(Into::into)
        }
      }

      impl CustomInto<Vec<$name>> for Vec<proto::$name> {
        fn custom_into(self) -> Vec<$name> {
          self.into_iter().map(Into::into).collect()
        }
      }

      impl CustomInto<Vec<proto::$name>> for Vec<$name> {
        fn custom_into(self) -> Vec<proto::$name> {
          self.into_iter().map(Into::into).collect()
        }
      }
    )+
  };
}

// custom_into!(Device DevicePluginOptions NumaNode TopologyInfo);

macro_rules! derive_to_from_proto {
  ($name:ident {
    $($mapping:tt)+
   }) => {
    impl From<proto::$name> for $name {
      fn from(value: proto::$name) -> Self {
        derive_to_from_proto!(@from_proto value [$($mapping)+] [])
      }
    }

    impl From<$name> for proto::$name {
      fn from(value: $name) -> Self {
        derive_to_from_proto!(@to_proto value [$($mapping)+] [])
      }
    }

    custom_into!{$name}
  };

  (@from_proto $value:ident [$fld:ident $(,)*] [$($acc:tt)*]) => {
    Self { $fld: $value.$fld.custom_into(), $($acc)* }
  };

  (@from_proto $value:ident [$fld:ident:$alias:ident $(,)*] [$($acc:tt)*]) => {
    Self { $fld: $value.$alias.custom_into(), $($acc)* }
  };

  (@from_proto $value:ident [$fld:ident, $($mapping:tt)+] [$($acc:tt)*]) => {
    derive_to_from_proto!(@from_proto $value [$($mapping)+] [$($acc)* $fld: $value.$fld.custom_into(),])
  };

  (@from_proto $value:ident [$fld:ident:$alias:ident, $($mapping:tt)+] [$($acc:tt)*]) => {
    derive_to_from_proto!(@from_proto $value [$($mapping)+] [$($acc)* $fld: $value.$alias.custom_into(),])
  };

  (@to_proto $value:ident [$fld:ident $(,)*] [$($acc:tt)*]) => {
    Self { $fld: $value.$fld.custom_into(), $($acc)* }
  };

  (@to_proto $value:ident [$fld:ident:$alias:ident $(,)*] [$($acc:tt)*]) => {
    Self { $alias: $value.$fld.custom_into(), $($acc)* }
  };

  (@to_proto $value:ident [$fld:ident, $($mapping:tt)+] [$($acc:tt)*]) => {
    derive_to_from_proto!(@to_proto $value [$($mapping)+] [$($acc)* $fld: $value.$fld.custom_into(),])
  };

  (@to_proto $value:ident [$fld:ident:$alias:ident, $($mapping:tt)+] [$($acc:tt)*]) => {
    derive_to_from_proto!(@to_proto $value [$($mapping)+] [$($acc)* $alias: $value.$fld.custom_into(),])
  };
}

// #[derive(Debug, Clone)]
// pub struct DevicePluginOptions {
//   /// Indicates if PreStartContainer call is required before each container start
//   pub pre_start_required: bool,

//   /// Indicates if GetPreferredAllocation is implemented and available for calling
//   pub get_preferred_allocation_available: bool,
// }

// //trace_macros!(true);
// derive_to_from_proto!(DevicePluginOptions {
//   pre_start_required,
//   get_preferred_allocation_available,
// });

// #[derive(Debug, Clone)]
// pub struct RegisterRequest {
//   /// Version of the API the Device Plugin was built against
//   pub version: String,
//   /// Name of the unix socket the device plugin is listening on
//   /// PATH = path.Join(DevicePluginPath, endpoint)
//   pub endpoint: String,
//   /// Schedulable resource name. As of now it's expected to be a DNS Label
//   pub resource_name: String,
//   /// Options to be communicated with Device Manager
//   pub options: Option<DevicePluginOptions>,
// }
// derive_to_from_proto!(RegisterRequest {
//   version,
//   endpoint,
//   resource_name,
//   options,
// });

/// ListAndWatch returns a stream of List of Devices
/// Whenever a Device state change or a Device disappears, ListAndWatch
/// returns the new list
#[derive(Debug, Clone)]
pub struct ListAndWatchResponse {
  pub devices: Vec<Device>,
}
derive_to_from_proto!(ListAndWatchResponse { devices });

#[derive(Debug, Clone)]
pub struct TopologyInfo {
  pub nodes: Vec<NumaNode>,
}
derive_to_from_proto!(TopologyInfo { nodes });

#[derive(Debug, Clone)]
pub struct NumaNode {
  pub id: i64,
}
derive_to_from_proto!(NumaNode { id });

/// E.g:
/// ```pseudocode
/// struct Device {
///    ID: "GPU-fef8089b-4820-abfc-e83e-94318197576e",
///    Health: "Healthy",
///    Topology:
///      Node:
///        ID: 1
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Device {
  /// A unique ID assigned by the device plugin used
  /// to identify devices during the communication
  /// Max length of this field is 63 characters
  pub id: String,
  /// Health of the device, can be healthy or unhealthy, see constants.go
  pub health: DeviceHealth,
  /// Topology for device
  pub topology: Option<TopologyInfo>,
}
derive_to_from_proto!(Device {
  id,
  health,
  topology,
});

#[derive(Debug, Clone)]
pub enum DeviceHealth {
  Healthy,
  Unhealthy,
  Other(String),
}

impl CustomInto<DeviceHealth> for String {
  fn custom_into(self) -> DeviceHealth {
    match &*self {
      super::HEALTHY => DeviceHealth::Healthy,
      super::UNHEALTHY => DeviceHealth::Unhealthy,
      _ => DeviceHealth::Other(self),
    }
  }
}
impl CustomInto<String> for DeviceHealth {
  fn custom_into(self) -> String {
    match self {
      DeviceHealth::Healthy => super::HEALTHY.into(),
      DeviceHealth::Unhealthy => super::UNHEALTHY.into(),
      DeviceHealth::Other(v) => v,
    }
  }
}

/// - PreStartContainer is expected to be called before each container start if indicated by plugin during registration phase.
/// - PreStartContainer allows kubelet to pass reinitialized devices to containers.
/// - PreStartContainer allows Device Plugin to run device specific operations on
///   the Devices requested
#[derive(Debug, Clone)]
pub struct PreStartContainerRequest {
  pub devices_ids: Vec<String>,
}
derive_to_from_proto!(PreStartContainerRequest {
  devices_ids: devices_i_ds,
});

/// PreferredAllocationRequest is passed via a call to GetPreferredAllocation()
/// at pod admission time. The device plugin should take the list of
/// `available_deviceIDs` and calculate a preferred allocation of size
/// 'allocation_size' from them, making sure to include the set of devices
/// listed in 'must_include_deviceIDs'.
#[derive(Debug, Clone)]
pub struct PreferredAllocationRequest {
  pub container_requests: Vec<ContainerPreferredAllocationRequest>,
}
derive_to_from_proto!(PreferredAllocationRequest { container_requests });

#[derive(Debug, Clone)]
pub struct ContainerPreferredAllocationRequest {
  /// List of available deviceIDs from which to choose a preferred allocation
  pub available_device_ids: Vec<String>,
  /// List of deviceIDs that must be included in the preferred allocation
  pub must_include_device_ids: Vec<String>,
  /// Number of devices to include in the preferred allocation
  pub allocation_size: i32,
}
derive_to_from_proto!(ContainerPreferredAllocationRequest {
  available_device_ids: available_device_i_ds,
  must_include_device_ids: must_include_device_i_ds,
  allocation_size,
});

/// PreferredAllocationResponse returns a preferred allocation,
/// resulting from a PreferredAllocationRequest.
#[derive(Debug, Clone)]
pub struct PreferredAllocationResponse {
  pub container_responses: Vec<ContainerPreferredAllocationResponse>,
}
derive_to_from_proto!(PreferredAllocationResponse {
  container_responses,
});

#[derive(Debug, Clone)]
pub struct ContainerPreferredAllocationResponse {
  pub device_ids: Vec<String>,
}
derive_to_from_proto!(ContainerPreferredAllocationResponse {
  device_ids: device_i_ds,
});

/// - Allocate is expected to be called during pod creation since allocation
///   failures for any container would result in pod startup failure.
/// - Allocate allows kubelet to exposes additional artifacts in a pod's
///   environment as directed by the plugin.
/// - Allocate allows Device Plugin to run device specific operations on
///   the Devices requested
#[derive(Debug, Clone)]
pub struct AllocateRequest {
  pub container_requests: Vec<ContainerAllocateRequest>,
}
derive_to_from_proto!(AllocateRequest { container_requests });
#[derive(Debug, Clone)]
pub struct ContainerAllocateRequest {
  pub devices_ids: Vec<String>,
}
derive_to_from_proto!(ContainerAllocateRequest {
  devices_ids: devices_i_ds,
});

/// AllocateResponse includes the artifacts that needs to be injected into
/// a container for accessing 'deviceIDs' that were mentioned as part of
/// 'AllocateRequest'.
/// Failure Handling:
/// if Kubelet sends an allocation request for dev1 and dev2.
/// Allocation on dev1 succeeds but allocation on dev2 fails.
/// The Device plugin should send a ListAndWatch update and fail the
/// Allocation request
#[derive(Debug, Clone)]
pub struct AllocateResponse {
  pub container_responses: Vec<ContainerAllocateResponse>,
}
derive_to_from_proto!(AllocateResponse {
  container_responses,
});

#[derive(Debug, Clone)]
pub struct ContainerAllocateResponse {
  /// List of environment variable to be set in the container to access one of more devices.
  pub envs: HashMap<String, String>,
  /// Mounts for the container.
  pub mounts: Vec<Mount>,
  /// Devices for the container.
  pub devices: Vec<DeviceSpec>,
  /// Container annotations to pass to the container runtime
  pub annotations: HashMap<String, String>,
}
derive_to_from_proto!(ContainerAllocateResponse {
  envs,
  mounts,
  devices,
  annotations,
});

/// Mount specifies a host volume to mount into a container.
/// where device library or tools are installed on host and container
#[derive(Debug, Clone)]
pub struct Mount {
  /// Path of the mount within the container.
  pub container_path: String,
  /// Path of the mount on the host.
  pub host_path: String,
  /// If set, the mount is read-only.
  pub read_only: bool,
}
derive_to_from_proto!(Mount {
  container_path,
  host_path,
  read_only,
});

/// DeviceSpec specifies a host device to mount into a container.
#[derive(Debug, Clone)]
pub struct DeviceSpec {
  /// Path of the device within the container.
  pub container_path: String,
  /// Path of the device on the host.
  pub host_path: String,
  /// Cgroups permissions of the device, candidates are one or more of
  /// * r - allows container to read from the specified device.
  /// * w - allows container to write to the specified device.
  /// * m - allows container to create device files that do not yet exist.
  pub permissions: String,
}
derive_to_from_proto!(DeviceSpec {
  container_path,
  host_path,
  permissions,
});
