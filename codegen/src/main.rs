use std::{
  fs,
  path::{Path, PathBuf},
};

use anyhow::Result;
use duct::cmd;
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, Copy, Hash, PartialEq, Eq)]
struct LocateProject<'a> {
  root: &'a str,
}

pub fn find_workspace() -> Result<PathBuf> {
  let json = cmd!("cargo", "locate-project", "--workspace").read()?;

  let parsed = serde_json::from_str::<LocateProject>(&json)?;
  let path: &Path = parsed.root.as_ref();
  Ok(path.parent().unwrap().to_owned())
}

fn main() -> Result<()> {
  let root = find_workspace()?;
  let proto_dir = root.join("proto");
  let vendor_dir = root.join("vendor");
  let target_dir = root.join("target").join("proto");
  let csi_proto_file = proto_dir.join("v1beta1.proto");

  fs::create_dir_all(&target_dir)?;

  let proto_crate_src_dir = root.join("crates").join("proto").join("src");
  let mut config = prost_build::Config::default();
  config.protoc_arg(&format!("-I={}", proto_dir.display()));
  config.protoc_arg(&format!("-I={}", vendor_dir.display()));

  tonic_build::configure()
    .out_dir(&target_dir)
    .build_client(true)
    .build_server(true)
    .compile_with_config(config, &[csi_proto_file], &[])?;

  let csi_file = target_dir.join("v1beta1.rs");
  let target_file = proto_crate_src_dir.join("v1beta1/proto.rs");

  if target_file.is_file() {
    fs::remove_file(&target_file)?;
  }

  fs::copy(&csi_file, &target_file)?;

  Ok(())
}
