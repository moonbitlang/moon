use std::io::Write;
use std::path::Path;

use anyhow::Context;
use colored::Colorize;

use moonutil::common::MOON_PKG_JSON;
use moonutil::module::MoonModJSON;
use moonutil::package::MoonPkgJSON;
use moonutil::package::PkgJSONImportItem;

use moonutil::common::MOON_MOD_JSON;

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
    license: String,
) -> anyhow::Result<i32> {
    let cake_full_name = format!("{}/{}", user, name);
    common(target_dir, &cake_full_name, license)?;

    let main_dir = target_dir.join("main");
    create_or_warning(&main_dir)?;
    // main/${MOON_PKG}
    {
        let main_moon_pkg = main_dir.join(MOON_PKG_JSON);
        let j = MoonPkgJSON {
            name: None,
            is_main: Some(true),
            import: Some(moonutil::package::PkgJSONImport::List(vec![
                PkgJSONImportItem::String(format!("{}/lib", cake_full_name)),
            ])),
            test_import: None,
            link: None,
            warn_list: None,
            alert_list: None,
        };
        moonutil::common::write_package_json_to_file(&j, &main_moon_pkg)?;
    }
    // main/main.mbt
    {
        let main_moon = main_dir.join("main.mbt");
        let content = r#"fn main {
  println(@lib.hello())
}
"#;
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
    license: String,
) -> anyhow::Result<i32> {
    let cake_full_name = format!("{}/{}", user, name);
    common(target_dir, &cake_full_name, license)?;

    // top.mbt
    {
        let top_mbt = target_dir.join("top.mbt");
        let content = r#"pub fn greeting() -> Unit {
  println(@lib.hello())
}
"#;
        let mut file = std::fs::File::create(top_mbt).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    // moon.pkg.json
    {
        let moon_pkg_json = target_dir.join("moon.pkg.json");
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

fn common(target_dir: &Path, cake_full_name: &String, license: String) -> anyhow::Result<i32> {
    std::fs::create_dir_all(target_dir).context("failed to create target directory")?;
    let git_init = std::process::Command::new("git")
        .arg("init")
        .current_dir(target_dir)
        .status();
    match git_init {
        Ok(status) if status.success() => {}
        _ => {
            eprintln!(
                "{}: git init failed, make sure you have git in PATH",
                "Warning".yellow().bold()
            );
        }
    }

    {
        let m: MoonModJSON = MoonModJSON {
            name: cake_full_name.clone(),
            version: Some("0.1.0".parse().unwrap()),
            deps: None,
            readme: Some("README.md".into()),
            repository: Some("".into()),
            license: Some(license),
            keywords: Some(vec![]),
            description: Some("".into()),

            compile_flags: None,
            link_flags: None,
            checksum: None,
            ext: Default::default(),
        };
        moonutil::common::write_module_json_to_file(&m, target_dir)
            .context(format!("failed to write `{}`", MOON_MOD_JSON))?;
    }
    // .gitignore
    {
        let gitignore = target_dir.join(".gitignore");
        let content = "target/\n.mooncakes/\n";
        let mut file = std::fs::File::create(gitignore).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }
    let lib_dir = target_dir.join("lib");
    create_or_warning(&lib_dir)?;
    // lib/hello.mbt
    {
        let hello_mbt = lib_dir.join("hello.mbt");
        let content = r#"pub fn hello() -> String {
  "Hello, world!"
}
"#;
        let mut file = std::fs::File::create(hello_mbt).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }
    // lib/hello_test.mbt
    {
        let hello_mbt = lib_dir.join("hello_test.mbt");
        let content = r#"test "hello" {
  if hello() != "Hello, world!" {
    return Err("hello() != \"Hello, world!\"")
  }
}
"#;

        let mut file = std::fs::File::create(hello_mbt).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }
    // lib/moon.pkg.json
    {
        let lib_moon_pkg = lib_dir.join(MOON_PKG_JSON);
        let j = MoonPkgJSON {
            name: None,
            is_main: None,
            import: None,
            test_import: None,
            link: None,
            warn_list: None,
            alert_list: None,
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

    Ok(0)
}
