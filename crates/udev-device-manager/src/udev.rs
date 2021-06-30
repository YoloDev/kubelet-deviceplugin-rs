mod device;
mod event_stream;

use event_stream::UdevEventStreamBuilder;
use futures::Stream;

pub use device::{UdevDevice, UdevDeviceError};
pub use event_stream::{UdevBuilderError, UdevEvent};

pub struct Udev;

impl Udev {
  pub async fn watch(
  ) -> Result<impl Stream<Item = Result<UdevEvent, UdevDeviceError>>, UdevBuilderError> {
    UdevEventStreamBuilder::new()?.listen().await
  }
}
