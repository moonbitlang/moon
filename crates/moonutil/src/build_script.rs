use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::common::TargetBackend;

/// Represents the environment a build script receives
#[derive(Serialize, Deserialize)]
pub struct BuildScriptEnvironment {
    build: BuildInfo,
    env: HashMap<String, String>,
    paths: Paths,
}

#[derive(Serialize, Deserialize)]
pub struct BuildInfo {
    /// The profile we're building with, e.g. `debug`, `release`.
    profile: String,
    /// The target info for the build script currently being run.
    host: TargetInfo,
    /// The target info for the module being built.
    target: TargetInfo,
}

#[derive(Serialize, Deserialize)]
pub struct TargetInfo {
    // this is mostly useless now unless we're using the native backends, but
    // this will buy us some wiggle room in the future when very cross-y cross
    // compilation becomes a thing
    /// The actual backend we're using, e.g. `wasm32`, `wasmgc`, `js`, `c`,
    /// `llvm`
    kind: TargetBackend,

    /// The architecture of the target. This is either the architecture in the
    /// target triple like `x86_64` and `aarch64`, or one of our other
    /// non-native backends like `js`, `wasm32` and `wasmgc`.
    arch: String,
    /// The vendor of the target. This is often `unknown`.
    vendor: String,
    /// The operating system of the target. This is often `linux`, `windows` or
    /// `macos`.
    os: String,
    /// The ABI of the target. Might be null, or something like `gnu`, `musl`,
    /// `msvc`, `eabi` and similar.
    abi: Option<String>,
    /// The target triple, e.g. `x86_64-unknown-linux-gnu`. This also applies to
    /// the non-native backends like `js-unknown-unknown`. This field is
    /// essentially arch+vendor+os+abi, but makes matching easier.
    triplet: String,
}

#[derive(Serialize, Deserialize)]
pub struct Paths {
    /// The directory containing the current module, i.e. the parent directory
    /// of `moon.mod.json`.
    module_root: String,
    /// The directory that the build script can write to. This directory resides
    /// within the `target` directory of the project building this module.
    out_dir: String,
}

/// The output of the build script. This is usually one message per line.
#[derive(Serialize, Deserialize)]
pub enum BuildScriptOutput {
    /// Rerun the build script if the file at the given path changes.
    RerunIf { cond: RerunIfKind },

    /// Set an environment variable to the given value.
    SetEnv {
        /// The name of the environment variable to set.
        name: String,
        /// The value of the environment variable to set.
        value: String,
    },
}

#[derive(Serialize, Deserialize)]
pub enum RerunIfKind {
    /// Rerun if the file at the given path changes.
    File(String),
    /// Rerun if the directory at the given path changes.
    Dir(String),
    /// Rerun if the environment variable with the given name changes.
    Env(String),
}
