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
    println!("cargo:rustc-env=CARGO_PKG_VERSION=0.1.{date_str}");

    println!("cargo:rerun-if-changed=resources/error_codes");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("error_code_docs.rs");

    let error_docs_map = collect_docs_map(
        Path::new("resources/error_codes/language/error_codes"),
        |file_name| {
            let code = file_name.strip_prefix('E')?.strip_suffix(".md")?;
            code.chars()
                .all(|ch| ch.is_ascii_digit())
                .then(|| code.to_owned())
        },
    );
    let attribute_docs_map = collect_docs_map(
        Path::new("resources/error_codes/language/attributes"),
        |file_name| file_name.strip_suffix(".md").map(ToOwned::to_owned),
    );

    fs::write(
        dest_path,
        format!(
            "pub static ERROR_DOCS: std::sync::LazyLock<HashMap<&'static str, &'static str>> = std::sync::LazyLock::new(|| {error_docs_map});\n\
             pub static ATTRIBUTE_DOCS: std::sync::LazyLock<HashMap<&'static str, &'static str>> = std::sync::LazyLock::new(|| {attribute_docs_map});"
        ),
    )
    .unwrap();

    Ok(())
}

fn collect_docs_map(docs_dir: &Path, key_for_file: impl Fn(&str) -> Option<String>) -> String {
    let mut docs = Vec::new();
    if let Ok(entries) = fs::read_dir(docs_dir) {
        for entry in entries.flatten() {
            if let Some(file_name) = entry.file_name().to_str()
                && let Some(key) = key_for_file(file_name)
                && let Ok(content) = fs::read_to_string(entry.path())
            {
                docs.push((key, content));
            }
        }
    }
    docs.sort_by(|(left, _), (right, _)| left.cmp(right));

    let mut docs_map = String::from("{\n    #[allow(unused_mut)] let mut m = HashMap::new();\n");
    for (key, content) in docs {
        docs_map.push_str(&format!("    m.insert({key:?}, {content:?});\n"));
    }
    docs_map.push_str("    m\n}");
    docs_map
}
