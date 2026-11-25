#!/usr/bin/env bash
# manifest.sh <repo root> <out dir>: write the baseline crate. The patch tables come from the
# repository's own Cargo.toml, so run it at the commit the record names.
set -euo pipefail
root=$(realpath "$1"); here=$(dirname "$(realpath "$0")")
mkdir -p "$2/src"; out=$(realpath "$2")
cp "$here/main.rs" "$out/src/main.rs"; cp "$root/Cargo.lock" "$out/Cargo.lock"
{
  cat <<T
[package]
name = "baseline"
version = "0.0.0"
edition = "2024"
publish = false
[workspace]
[dependencies]
openhuman-embed = { git = "https://github.com/tinyhumansai/openhuman", rev = "5f8b7c235f18b46f90fabbf1e7f1b8d9a957cee6", default-features = false, features = ["mcp","skills","storage-sqlite","secret-file","legacy-import","testkit"] }
tinyhivemind-openhuman = { path = "$root/crates/tinyhivemind-openhuman", features = ["offline"] }
tinytools = { git = "https://github.com/tinyhumansai/tinytools", rev = "75e42ba9647eb801891a12ad05b6337f6a1c126b" }
async-trait = "0.1"
anyhow = "1"
serde_json = "1"
tokio = { version = "1", features = ["rt-multi-thread","macros","time"] }
[profile.release]
lto = "thin"
codegen-units = 1
T
  sed -n '/^\[patch/,$p' "$root/Cargo.toml" | sed "s#path = \"vendor/#path = \"$root/vendor/#"
} > "$out/Cargo.toml"
