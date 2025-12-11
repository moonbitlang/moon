// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use chrono::DateTime;
use std::{
    error::Error,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn main() -> Result<(), Box<dyn Error>> {
    // Emit: git info and build date
    vergen_git2::Emitter::new()
        .add_instructions(
            &vergen_git2::BuildBuilder::default()
                .build_date(true)
                .build()?,
        )?
        .add_instructions(&vergen_git2::Git2Builder::default().sha(true).build()?)?
        .emit()?;

    let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let datetime = DateTime::from_timestamp(time.as_secs() as i64, 0).unwrap();
    let date_str = datetime.format("%Y%m%d").to_string();
    println!("cargo:rustc-env=CARGO_PKG_VERSION=0.1.{date_str}");
    Ok(())
}
