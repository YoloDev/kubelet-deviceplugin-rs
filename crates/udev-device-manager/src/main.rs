mod app;
mod config;
mod signals;
mod udev;
mod utils;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
  color_eyre::install()?;
  app::run().await
}
