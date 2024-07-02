use anyhow::{bail, Context};
use colored::Colorize;
use futures::future::try_join_all;
use mooncake::pkg::sync::auto_sync;
use moonutil::{
    common::{
        read_module_desc_file_in_dir, FileLock, MoonbuildOpt, MooncOpt, RunMode, MOON_MOD_JSON,
    },
    dirs::PackageDirs,
    mooncakes::{sync::AutoSyncFlags, RegistryConfig},
};

use super::UniversalFlags;

/// Generate public interface (`.mbti`) files for all packages in the module
#[derive(Debug, clap::Parser)]
pub struct InfoSubcommand {
    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,
}

pub fn run_info(cli: UniversalFlags, cmd: InfoSubcommand) -> anyhow::Result<i32> {
    if cli.dry_run {
        bail!("dry-run is not implemented for info")
    }

    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let _lock = FileLock::lock(&target_dir)?;

    // Run moon install before build
    let (resolved_env, dir_sync_result) = auto_sync(
        &source_dir,
        &cmd.auto_sync_flags,
        &RegistryConfig::load(),
        cli.quiet,
    )?;

    let mod_desc = read_module_desc_file_in_dir(&source_dir).with_context(|| {
        format!(
            "failed to read module description file: {}",
            source_dir
                .join(MOON_MOD_JSON)
                .display()
                .to_string()
                .bold()
                .red()
        )
    })?;
    let module_name = &mod_desc.name;
    let mut moonc_opt = MooncOpt::default();
    if module_name == "moonbitlang/core" {
        moonc_opt.nostd = true;
    }
    let moonbuild_opt = MoonbuildOpt {
        source_dir: source_dir.clone(),
        target_dir: target_dir.clone(),
        sort_input: false,
        run_mode: RunMode::Check,
        ..Default::default()
    };

    let module = moonutil::scan::scan(
        false,
        &resolved_env,
        &dir_sync_result,
        &moonc_opt,
        &moonbuild_opt,
    )?;
    let check_result = moonbuild::entry::run_check(&moonc_opt, &moonbuild_opt, &module);
    match check_result {
        Ok(0) => {}
        _ => {
            bail!("moon check failed");
        }
    }
    let mdb = moonutil::scan::scan(
        false,
        &resolved_env,
        &dir_sync_result,
        &moonc_opt,
        &MoonbuildOpt {
            source_dir: source_dir.clone(),
            target_dir,
            ..Default::default()
        },
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    let mut handlers = vec![];
    for (name, pkg) in mdb.packages {
        // Skip if pkg is not part of the module
        if pkg.is_third_party {
            continue;
        }

        let source_dir = std::sync::Arc::new(source_dir.clone());
        handlers.push(async move {
            let mi = pkg.artifact.with_extension("mi");
            if !mi.exists() {
                bail!("cannot find mi file for package {}", name);
            }

            let out = tokio::process::Command::new("mooninfo")
                .args(["-format=text", mi.display().to_string().as_str()])
                .output()
                .await?;

            if out.status.success() {
                let filename = format!("{}.mbti", pkg.last_name());
                let filepath = source_dir.join(pkg.rel.fs_full_name()).join(&filename);

                tokio::fs::write(filepath, out.stdout)
                    .await
                    .context(format!("failed to write {}", filename))?;
            } else {
                eprintln!("{}", String::from_utf8_lossy(&out.stderr));
                eprintln!("failed to run `mooninfo -format=text {}`", mi.display());
            }

            Ok(0)
        });
    }

    // `try_join_all` will return immediately if anyone task fail
    runtime.block_on(try_join_all(handlers))?;

    Ok(0)
}
