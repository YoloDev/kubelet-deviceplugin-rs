use crate::config;
use clap::Clap;
use std::path::PathBuf;

#[derive(Clap, Debug, PartialEq, Clone, Copy)]
pub enum LogFormat {
  Pretty,
  Json,
}

#[derive(Clap, Debug, PartialEq, Clone, Copy)]
pub enum ConfigFormat {
  Json,
  Yaml,
  Toml,
  Auto,
}

impl From<ConfigFormat> for config::ConfigFormat {
  fn from(f: ConfigFormat) -> Self {
    match f {
      ConfigFormat::Json => config::ConfigFormat::Json,
      ConfigFormat::Yaml => config::ConfigFormat::Yaml,
      ConfigFormat::Toml => config::ConfigFormat::Toml,
      ConfigFormat::Auto => config::ConfigFormat::Auto,
    }
  }
}

#[derive(Clap, Debug)]
pub struct Args {
  /// Log output format
  #[clap(
    arg_enum,
    long = "log-format",
    short = 'f',
    env = "LOG_FORMAT",
    default_value = "pretty"
  )]
  pub log_format: LogFormat,

  /// Config file format
  #[clap(
    arg_enum,
    long = "config-format",
    short = 't',
    env = "CONFIG_FILE_FORMAT",
    default_value = "auto"
  )]
  pub config_format: ConfigFormat,

  /// Configuration file path
  #[clap(long = "config", short = 'c', env = "CONFIG_FILE")]
  pub config_file: PathBuf,
}
