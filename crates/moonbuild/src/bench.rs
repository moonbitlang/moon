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

use indexmap::IndexMap;
use moonutil::common::*;
use moonutil::module::MoonModJSON;
use moonutil::package::{MoonPkgJSON, PkgJSONImport};
use std::fs;
use std::path::Path;

#[derive(Default)]
pub struct Config {
    pub dir_rows: u32,
    pub dir_cols: u32,
    pub mod_rows: u32,
    pub mod_cols: u32,
    comment_size: u32,
}

impl Config {
    pub fn new() -> Self {
        Config {
            dir_rows: 1,
            dir_cols: 1,
            mod_rows: 1,
            mod_cols: 1,
            comment_size: 5000,
        }
    }
}

fn write_directory(config: &Config, base_dir: &Path, dr: u32, dc: u32) {
    let dir_name = base_dir.join(format!("dir_{}_{}", dr, dc));
    fs::create_dir_all(&dir_name).unwrap();

    for mr in 0..config.mod_rows {
        for mc in 0..config.mod_cols {
            let mut deps = Vec::new();
            if mr == 0 {
                if dr == 0 {
                    // nothing to do
                } else {
                    for k in 0..config.mod_cols {
                        for j in 0..config.dir_cols {
                            deps.push((dr - 1, j, config.mod_rows - 1, k));
                        }
                    }
                }
            } else {
                for k in 0..config.mod_rows {
                    deps.push((dr, dc, mr - 1, k));
                }
            }

            let mut dep_str = String::new();
            for d in deps.iter() {
                dep_str.push_str(&format!("  @m_{}_{}_{}_{}.f()\n", d.0, d.1, d.2, d.3));
            }

            let mod_content = format!(
                r#"// {} //
pub fn f() -> Unit {{
{}}}
"#,
                "X".repeat(config.comment_size as usize),
                dep_str,
            );
            let mod_name = dir_name.join(format!("m_{}_{}_{}_{}", dr, dc, mr, mc));
            fs::create_dir_all(&mod_name).unwrap();
            let mod_main = mod_name.join("main.mbt");
            fs::write(&mod_main, mod_content).unwrap();
            let moon_pkg = mod_name.join(MOON_PKG_JSON);
            let mut import: IndexMap<String, Option<String>> = IndexMap::new();
            for d in deps.iter() {
                import.insert(
                    format!(
                        "build_matrix/dir_{}_{}/m_{}_{}_{}_{}",
                        d.0, d.1, d.0, d.1, d.2, d.3
                    ),
                    Some("".into()),
                );
            }
            let pkg = MoonPkgJSON {
                name: None,
                is_main: None,
                import: if import.is_empty() {
                    None
                } else {
                    Some(PkgJSONImport::Map(import))
                },
                wbtest_import: None,
                test_import: None,
                test_import_all: None,
                link: None,
                warn_list: None,
                alert_list: None,
                targets: None,
                pre_build: None,
                post_build: None,
                bin_name: None,
                bin_target: None,
                supported_targets: None,
                native_stub: None,
            };
            moonutil::common::write_package_json_to_file(&pkg, &moon_pkg).unwrap();
        }
    }
}

pub fn write(config: &Config, base_dir: &Path) {
    for row in 0..config.dir_rows {
        for col in 0..config.dir_cols {
            write_directory(config, base_dir, row, col);
        }
    }

    let module = MoonModJSON {
        name: "build_matrix".to_string(),
        version: None,
        deps: None,
        bin_deps: None,
        readme: None,
        repository: None,
        license: None,
        keywords: None,
        description: None,

        compile_flags: None,
        link_flags: None,
        checksum: None,
        source: None,
        ext: Default::default(),

        alert_list: None,
        warn_list: None,

        include: None,
        exclude: None,
    };
    moonutil::common::write_module_json_to_file(&module, base_dir).unwrap();
    fs::create_dir_all(base_dir.join("main")).unwrap();
    let mut main_content = String::new();
    main_content.push_str("fn main {\n");
    for dr in 0..config.dir_rows {
        for dc in 0..config.dir_cols {
            for mr in 0..config.mod_rows {
                for mc in 0..config.mod_cols {
                    main_content.push_str(&format!("    let _ = @m_{dr}_{dc}_{mr}_{mc}.f\n"));
                }
            }
        }
    }
    main_content.push_str("    println(\"ok\")\n");
    main_content.push_str("}\n");

    fs::write(base_dir.join("main").join("main.mbt"), main_content).unwrap();

    let mut import = IndexMap::new();
    for dr in 0..config.dir_rows {
        for dc in 0..config.dir_cols {
            for mr in 0..config.mod_rows {
                for mc in 0..config.mod_cols {
                    import.insert(
                        format!(
                            "build_matrix/dir_{}_{}/m_{}_{}_{}_{}",
                            dr, dc, dr, dc, mr, mc
                        ),
                        Some("".to_string()),
                    );
                }
            }
        }
    }

    let pkg = MoonPkgJSON {
        name: None,
        is_main: Some(true),
        import: if import.is_empty() {
            None
        } else {
            Some(PkgJSONImport::Map(import))
        },
        wbtest_import: None,
        test_import: None,
        test_import_all: None,
        link: None,
        warn_list: None,
        alert_list: None,
        targets: None,
        pre_build: None,
        post_build: None,
        bin_name: None,
        bin_target: None,
        supported_targets: None,
        native_stub: None,
    };

    moonutil::common::write_package_json_to_file(&pkg, &base_dir.join("main").join(MOON_PKG_JSON))
        .unwrap();
}
