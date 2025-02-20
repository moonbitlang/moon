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

use std::io::Write;
use std::path::Path;

use anyhow::Context;
use colored::Colorize;

use moonutil::common::MOON_PKG_JSON;
use moonutil::module::MoonModJSON;
use moonutil::package::MoonPkgJSON;
use moonutil::package::PkgJSONImportItem;

use moonutil::common::MOON_MOD_JSON;

use moonutil::git::{git_init_repo, is_in_git_repo};

pub fn create_or_warning(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        eprintln!(
            "{} {}",
            "Warning:".bold().yellow(),
            format_args!("{} already exists", path.display())
        );
    } else {
        std::fs::create_dir_all(path).context(format!("failed to create {}", path.display()))?;
    }
    Ok(())
}

pub fn moon_new_exec(
    target_dir: &Path,
    user: String,
    name: String,
    license: Option<&str>,
) -> anyhow::Result<i32> {
    let cake_full_name = format!("{}/{}", user, name);
    let source = target_dir.join("src");
    common(target_dir, &source, &cake_full_name, license)?;

    let main_dir = source.join("main");
    create_or_warning(&main_dir)?;
    // src/main/${MOON_PKG}
    {
        let main_moon_pkg = main_dir.join(MOON_PKG_JSON);
        let j = MoonPkgJSON {
            name: None,
            is_main: Some(true),
            import: Some(moonutil::package::PkgJSONImport::List(vec![
                PkgJSONImportItem::String(format!("{}/lib", cake_full_name)),
            ])),
            wbtest_import: None,
            test_import: None,
            test_import_all: None,
            link: None,
            warn_list: None,
            alert_list: None,
            targets: None,
            pre_build: None,
            bin_name: None,
            bin_target: None,
            supported_targets: None,
            native_stub: None,
        };
        moonutil::common::write_package_json_to_file(&j, &main_moon_pkg)?;
    }
    // src/main/main.mbt
    {
        let main_moon = main_dir.join("main.mbt");
        let content = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../moonbuild/template/moon_new_template/main.mbt"
        ));

        let mut file = std::fs::File::create(main_moon).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    println!("{} {}", "Created".bold().green(), target_dir.display());

    Ok(0)
}

pub fn moon_new_lib(
    target_dir: &Path,
    user: String,
    name: String,
    license: Option<&str>,
) -> anyhow::Result<i32> {
    let cake_full_name = format!("{}/{}", user, name);
    let source = target_dir.join("src");
    common(target_dir, &source, &cake_full_name, license)?;

    // src/top.mbt
    {
        let top_mbt = source.join("top.mbt");
        let content = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../moonbuild/template/moon_new_template/top.mbt"
        ));
        let mut file = std::fs::File::create(top_mbt).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    // src/moon.pkg.json
    {
        let moon_pkg_json = source.join("moon.pkg.json");
        let content = format!(
            r#"{{
  "import": [
    "{}/lib"
  ]
}}
"#,
            cake_full_name
        );
        let mut file = std::fs::File::create(moon_pkg_json).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    println!("{} {}", "Created".bold().green(), target_dir.display());

    Ok(0)
}

fn common(
    target_dir: &Path,
    source: &Path,
    cake_full_name: &str,
    license: Option<&str>,
) -> anyhow::Result<i32> {
    std::fs::create_dir_all(target_dir).context("failed to create target directory")?;

    if !is_in_git_repo(target_dir)? {
        git_init_repo(target_dir)?;
    }

    {
        let m: MoonModJSON = MoonModJSON {
            name: cake_full_name.into(),
            version: Some("0.1.0".parse().unwrap()),
            deps: None,
            bin_deps: None,
            readme: Some("README.md".into()),
            repository: Some("".into()),
            license: license
                .map(|s| s.to_string())
                .or_else(|| Some(String::from(""))),
            keywords: Some(vec![]),
            description: Some("".into()),

            compile_flags: None,
            link_flags: None,
            checksum: None,
            source: Some("src".to_string()),
            ext: Default::default(),

            alert_list: None,
            warn_list: None,

            include: None,
            exclude: None,
        };
        moonutil::common::write_module_json_to_file(&m, target_dir)
            .context(format!("failed to write `{}`", MOON_MOD_JSON))?;
    }
    // .gitignore
    {
        let gitignore = target_dir.join(".gitignore");
        let content = ["target/", ".mooncakes/", ".DS_Store"];
        let content = content.join("\n") + "\n";
        let mut file = std::fs::File::create(gitignore).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }
    let lib_dir = source.join("lib");
    create_or_warning(&lib_dir)?;
    // src/lib/hello.mbt
    {
        let hello_mbt = lib_dir.join("hello.mbt");
        let content = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../moonbuild/template/moon_new_template/hello.mbt"
        ));
        let mut file = std::fs::File::create(hello_mbt).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }
    // src/lib/hello_test.mbt
    {
        let hello_mbt = lib_dir.join("hello_test.mbt");
        let content = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../moonbuild/template/moon_new_template/hello_test.mbt"
        ));

        let mut file = std::fs::File::create(hello_mbt).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }
    // src/lib/moon.pkg.json
    {
        let lib_moon_pkg = lib_dir.join(MOON_PKG_JSON);
        let j = MoonPkgJSON {
            name: None,
            is_main: None,
            import: None,
            wbtest_import: None,
            test_import: None,
            test_import_all: None,
            link: None,
            warn_list: None,
            alert_list: None,
            targets: None,
            pre_build: None,
            bin_name: None,
            bin_target: None,
            supported_targets: None,
            native_stub: None,
        };
        moonutil::common::write_package_json_to_file(&j, &lib_moon_pkg)?;
    }

    // READMD.md
    {
        let md_file = target_dir.join("README.md");
        let content = format!("# {}", cake_full_name);
        let mut file = std::fs::File::create(md_file).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    // LICENSE
    {
        if let Some("Apache-2.0") = license {
            let license_file = target_dir.join("LICENSE");
            let content = include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../moonbuild/template/apache-2.0.txt"
            ));
            let mut file = std::fs::File::create(license_file).unwrap();
            file.write_all(content.as_bytes()).unwrap();
        }
    }

    Ok(0)
}
