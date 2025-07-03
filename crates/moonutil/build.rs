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
use std::{env, fs, path::Path};
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
    println!("cargo:rustc-env=CARGO_PKG_VERSION=0.1.{}", date_str);

    println!("cargo:rerun-if-changed=resources/error_codes");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("error_code_docs.rs");

    let mut docs_map = String::from("{\n    let mut m = HashMap::new();\n");

    let docs_dir = Path::new("resources/error_codes/next/language/error_codes");
    if let Ok(entries) = fs::read_dir(docs_dir) {
        for entry in entries.flatten() {
            if let Some(file_name) = entry.file_name().to_str() {
                if file_name.ends_with(".md") {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        let error_code = file_name.trim_end_matches(".md").replace("E", "");
                        docs_map.push_str(&format!(
                            "    m.insert(\"{}\", r#\"{}\"#);\n",
                            error_code, content
                        ));
                    }
                }
            }
        }
    }

    docs_map.push_str("    m\n}");

    fs::write(
        dest_path,
        format!(
            "pub static ERROR_DOCS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {});",
            docs_map
        ),
    )
    .unwrap();

    Ok(())
}
