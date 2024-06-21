use indexmap::IndexMap;
use moonutil::common::*;
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
                test_import: None,
                link: None,
                warn_list: None,
                alert_list: None,
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
        readme: None,
        repository: None,
        license: None,
        keywords: None,
        description: None,

        compile_flags: None,
        link_flags: None,
        checksum: None,
        ext: Default::default(),
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
        test_import: None,
        link: None,
        warn_list: None,
        alert_list: None,
    };

    moonutil::common::write_package_json_to_file(&pkg, &base_dir.join("main").join(MOON_PKG_JSON))
        .unwrap();
}
