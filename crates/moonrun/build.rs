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

use chrono::DateTime;
use std::{
    error::Error,
    time::{SystemTime, UNIX_EPOCH},
};
use vergen::EmitBuilder;

pub fn main() -> Result<(), Box<dyn Error>> {
    EmitBuilder::builder().build_date().git_sha(true).emit()?;

    let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let datetime = DateTime::from_timestamp(time.as_secs() as i64, 0).unwrap();
    let date_str = datetime.format("%Y%m%d").to_string();
    println!("cargo:rustc-env=CARGO_PKG_VERSION=0.1.{date_str}");
    Ok(())
}
