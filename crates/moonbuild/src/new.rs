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

pub fn moon_new_default(
    target_dir: &Path,
    user: String,
    name: String,
    license: Option<&str>,
) -> anyhow::Result<i32> {
    let cake_full_name = format!("{user}/{name}");
    let short_name = name.rsplit_once('/').map_or(&*name, |(_, n)| n);
    common(target_dir, &cake_full_name, license)?;

    let cmd_dir = target_dir.join("cmd");
    create_or_warning(&cmd_dir)?;
    let cmd_main_dir = cmd_dir.join("main");
    create_or_warning(&cmd_main_dir)?;
    // cmd/main/${MOON_PKG}
    {
        let main_moon_pkg = cmd_main_dir.join(MOON_PKG_JSON);
        let j = MoonPkgJSON {
            name: None,
            is_main: Some(true),
            import: Some(moonutil::package::PkgJSONImport::List(vec![
                PkgJSONImportItem::Object {
                    path: cake_full_name,
                    alias: Some("lib".to_string()),
                    sub_package: None,
                    value: None,
                },
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
            virtual_pkg: None,
            implement: None,
            overrides: None,
            sub_package: None,
        };
        moonutil::common::write_package_json_to_file(&j, &main_moon_pkg)?;
    }
    // cmd/main/main.mbt
    {
        let main_moon = cmd_main_dir.join("main.mbt");
        let content = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../moonbuild/template/moon_new_template/main.mbt"
        ));

        let mut file = std::fs::File::create(main_moon).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }
    let lib_dir = target_dir;
    // <package>.mbt
    {
        let hello_mbt = lib_dir.join(format!("{short_name}.mbt"));
        let content = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../moonbuild/template/moon_new_template/hello.mbt"
        ));
        let mut file = std::fs::File::create(hello_mbt).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }
    // <package>_test.mbt
    {
        let hello_mbt = lib_dir.join(format!("{short_name}_test.mbt"));
        let content = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../moonbuild/template/moon_new_template/hello_test.mbt"
        ));

        let mut file = std::fs::File::create(hello_mbt).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }
    // <package>/moon.pkg.json
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
            virtual_pkg: None,
            implement: None,
            overrides: None,
            sub_package: None,
        };
        moonutil::common::write_package_json_to_file(&j, &lib_moon_pkg)?;
    }

    println!("{} {}", "Created".bold().green(), target_dir.display());

    Ok(0)
}

fn common(target_dir: &Path, cake_full_name: &str, license: Option<&str>) -> anyhow::Result<i32> {
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
            source: None,
            ext: Default::default(),

            alert_list: None,
            warn_list: None,

            include: None,
            exclude: None,

            scripts: None,
            preferred_target: None,

            __moonbit_unstable_prebuild: None,
        };
        moonutil::common::write_module_json_to_file(&m, target_dir)
            .context(format!("failed to write `{MOON_MOD_JSON}`"))?;
    }
    // .gitignore
    {
        let gitignore = target_dir.join(".gitignore");
        let content = [".DS_Store", "target/", ".mooncakes/", ".moonagent/"];
        let content = content.join("\n") + "\n";
        let mut file = std::fs::File::create(gitignore).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }
    // READMD.mbt.md
    {
        let md_file = target_dir.join("README.mbt.md");
        let content = format!("# {cake_full_name}");
        let mut file = std::fs::File::create(md_file).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }
    // README.md
    {
        let readme_file = target_dir.join("README.md");

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("README.mbt.md", &readme_file)
                .context("failed to create symbolic link to README.mbt.md")?;
        }

        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file("README.mbt.md", &readme_file)
                .context("failed to create symbolic link to README.mbt.md")?;
        }
    }
    // Agents.md
    {
        let agents_file = target_dir.join("Agents.md");
        let content = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../moonbuild/template/moon_new_template/Agents.md"
        ));
        let mut file = std::fs::File::create(agents_file).unwrap();
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
