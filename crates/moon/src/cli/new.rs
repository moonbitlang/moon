use std::path::PathBuf;

use anyhow::bail;
use colored::Colorize;
use moonutil::{common::MOON_MOD_JSON, mooncakes::validate_username};

use super::UniversalFlags;

/// Create a new moonbit package
#[derive(Debug, clap::Parser)]
pub struct NewSubcommand {
    /// The name of the package
    pub package_name: Option<String>,

    /// Create a library package instead of an executable
    #[clap(long)]
    pub lib: bool,

    #[clap(flatten)]
    pub path_user_name: NewPathUserName,

    /// The license of the package
    #[clap(long)]
    pub license: Option<String>,
}

#[derive(Debug, Clone, clap::Args)]
#[group(required(false), requires_all(["path", "user", "name"]))]
pub struct NewPathUserName {
    /// Output path of the package
    #[clap(long)]
    pub path: Option<PathBuf>,

    /// The user name of the package
    #[clap(long)]
    pub user: Option<String>,

    /// The name part of the package
    #[clap(long)]
    pub name: Option<String>,
}

pub fn run_new(_cli: &UniversalFlags, cmd: NewSubcommand) -> anyhow::Result<i32> {
    if _cli.dry_run {
        bail!("dry-run is not implemented for new")
    }

    let mut lib_flag = cmd.lib;
    let package_name = cmd.package_name.as_ref();
    if let Some(name) = package_name {
        let target_dir = PathBuf::from(name);
        if lib_flag {
            return moonbuild::new::moon_new_lib(
                &target_dir,
                "username".into(),
                "hello".into(),
                "".into(),
            );
        }

        return moonbuild::new::moon_new_exec(
            &target_dir,
            "username".into(),
            "hello".into(),
            "".into(),
        );
    }

    let NewPathUserName { path, user, name } = cmd.path_user_name;
    let license = cmd.license.as_deref();

    ctrlc::set_handler(moonutil::common::dialoguer_ctrlc_handler)?;

    let (target_dir, user, name, license) =
        if let (Some(path), Some(user), Some(name)) = (path, user, name) {
            (path, user, name, license.unwrap_or("").to_string())
        } else {
            let tmp = dialoguer::Input::<String>::new()
                .with_prompt("Enter the path to create the project (. for current directory)")
                .default("my-project".to_string())
                .show_default(true)
                .validate_with(|input: &String| -> Result<(), String> {
                    let p = input.trim();
                    let dot = p == ".";

                    let p = PathBuf::from(p);

                    if p.exists() {
                        if p.join(MOON_MOD_JSON).exists() {
                            Err(format!(
                                "A MoonBit project already exists in `{}`.",
                                p.display()
                            ))
                        } else {
                            if !dot {
                                eprintln!(
                                    "{}: The directory is already exists.",
                                    "Warning".yellow().bold(),
                                );
                            };
                            Ok(())
                        }
                    } else {
                        Ok(())
                    }
                })
                .interact()?;
            let path = PathBuf::from(tmp);

            let items = vec!["exec", "lib"];
            let selection = dialoguer::Select::new()
                .with_prompt("Select the create mode")
                .default(0)
                .items(&items)
                .interact()?;
            lib_flag = selection == 1;

            let username = dialoguer::Input::<String>::new()
                .with_prompt("Enter your username")
                .default("username".to_string())
                .validate_with(|input: &String| -> Result<(), String> { validate_username(input) })
                .show_default(true)
                .interact()?;

            let package_name = dialoguer::Input::<String>::new()
                .with_prompt("Enter your project name")
                .default("hello".to_string())
                .show_default(true)
                .interact()?;

            let license = dialoguer::Input::<String>::new()
                .with_prompt("Enter your license")
                .default("".to_string())
                .show_default(true)
                .interact()?;

            (path, username, package_name, license)
        };

    if target_dir.exists() && target_dir.join(MOON_MOD_JSON).exists() {
        bail!(
            "A MoonBit project already exists in `{}`.",
            target_dir.display()
        );
    }

    if !lib_flag {
        moonbuild::new::moon_new_exec(&target_dir, user, name, license)
    } else {
        moonbuild::new::moon_new_lib(&target_dir, user, name, license)
    }
}
