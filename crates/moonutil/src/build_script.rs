// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::target::TargetBackend;

/// Represents the environment a build script receives
#[derive(Serialize)]
pub struct BuildScriptEnvironment {
    // pub build: BuildInfo,
    pub env: HashMap<String, String>,
    pub paths: Paths,
}

pub struct BuildInfo {
    // /// The profile we're building with, e.g. `debug`, `release`.
    // pub profile: String,
    /// The target info for the build script currently being run.
    pub host: TargetInfo,
    /// The target info for the module being built.
    pub target: TargetInfo,
}

pub struct TargetInfo {
    // this is mostly useless now unless we're using the native backends, but
    // this will buy us some wiggle room in the future when very cross-y cross
    // compilation becomes a thing
    /// The actual backend we're using, e.g. `wasm32`, `wasmgc`, `js`, `c`,
    /// `llvm`
    pub kind: TargetBackend,

    /// The architecture of the target. This is either the architecture in the
    /// target triple like `x86_64` and `aarch64`, or one of our other
    /// non-native backends like `js`, `wasm32` and `wasmgc`.
    pub arch: String,
    /// The vendor of the target. This is often `unknown`.
    pub vendor: String,
    /// The operating system of the target. This is often `linux`, `windows` or
    /// `macos`.
    pub os: String,
    /// The ABI of the target. Might be null, or something like `gnu`, `musl`,
    /// `msvc`, `eabi` and similar.
    pub abi: Option<String>,
    /// The target triple, e.g. `x86_64-unknown-linux-gnu`. This also applies to
    /// the non-native backends like `js-unknown-unknown`. This field is
    /// essentially arch+vendor+os+abi, but makes matching easier.
    pub triplet: String,
}

#[derive(Serialize)]
pub struct Paths {
    /// The directory containing the current module, i.e. the parent directory
    /// of `moon.mod.json`.
    pub module_root: String,
    /// The directory that the build script can write to. This directory resides
    /// within the `target` directory of the project building this module.
    pub out_dir: String,
}

#[derive(Deserialize, Debug)]
pub struct BuildScriptOutput {
    /// Rerun conditions. **DOES NOT WORK NOW**
    #[serde(default)]
    pub rerun_if: Vec<RerunIfKind>,
    // TODO: How much of these vars are useful? We don't fetch link flags from
    // here any more. However, they might still be useful for future
    // match-replace in code.
    // TODO: what about array-like vars? like commandline args
    #[serde(default)]
    pub vars: HashMap<String, String>,
    #[serde(default)]
    /// Configurations to linking
    pub link_configs: Vec<LinkConfig>,
}

#[derive(Deserialize, Debug)]
pub enum RerunIfKind {
    /// Rerun if the file at the given path changes.
    File(String),
    /// Rerun if the directory at the given path changes.
    Dir(String),
    /// Rerun if the environment variable with the given name changes.
    Env(String),
}

#[derive(Deserialize, Debug)]
pub struct LinkConfig {
    pub package: String,
    // TODO: these are merely a POC, more polishing needed
    /// Link flags that needs to be propagated to dependents
    ///
    /// Reference: `cargo::rustc-link-arg=FLAG`
    #[serde(default)]
    pub link_flags: Option<String>,

    /// Libraries that need linking, propagated to dependents
    ///
    /// Reference: `cargo::rustc-link-lib=LIB`
    #[serde(default)]
    pub link_libs: Vec<String>,

    /// Paths that needs to be searched during linking, propagated to dependents
    ///
    /// `cargo::rustc-link-search=[KIND=]PATH`
    #[serde(default)]
    pub link_search_paths: Vec<String>,
}
