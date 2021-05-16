use std::{path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum DeviceStatus {
  Free,
  Taken,
}

#[derive(Debug)]
struct Device {
  id: String,
  path: PathBuf,
  status: DeviceStatus,
}

struct Inner {}

pub struct DeviceManager {
  inner: Arc<Mutex<Inner>>,
}
