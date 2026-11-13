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

use std::error::Error;
use time::{OffsetDateTime, format_description};
use vergen::EmitBuilder;

pub fn main() -> Result<(), Box<dyn Error>> {
    EmitBuilder::builder().build_date().git_sha(true).emit()?;

    if cfg!(feature = "v8") && std::env::var("TARGET")? == "x86_64-unknown-linux-gnu" {
        let source = "src/v8/memfd_create.c";
        println!("cargo:rerun-if-changed={source}");
        cc::Build::new().file(source).compile("v8_glibc_compat");
    }

    let date_str =
        OffsetDateTime::now_utc().format(&format_description::parse("[year][month][day]")?)?;
    println!("cargo:rustc-env=CARGO_PKG_VERSION=0.1.{date_str}");
    Ok(())
}
