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

use anyhow::Context;
use colored::*;
use mooncake::pkg::sync::auto_sync;
use moonutil::module::ModuleDB;
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::mooncakes::RegistryConfig;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use moonutil::common::{
    MoonbuildOpt, MooncOpt, RunMode, DOT_MBT_DOT_MD, MOON_MOD_JSON, MOON_PKG_JSON, WATCH_MODE_DIR,
};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub fn watching(
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    registry_config: &RegistryConfig,
    module: &ModuleDB,
    original_target_dir: &Path,
) -> anyhow::Result<i32> {
    run_and_print(moonc_opt, moonbuild_opt, module)?;

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;

    {
        // make sure the handler is only set once when --watch --target all
        static HANDLER_SET: AtomicBool = AtomicBool::new(false);

        if HANDLER_SET
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            ctrlc::set_handler(moonutil::common::dialoguer_ctrlc_handler)
                .expect("Error setting Ctrl-C handler");
        }
    }

    {
        // main thread
        watcher.watch(&moonbuild_opt.source_dir, RecursiveMode::Recursive)?;

        // in watch mode, moon is a long-running process that should handle errors as much as possible rather than throwing them up and then exiting.
        for res in rx {
            match res {
                Ok(event) => {
                    match event.kind {
                        // when a file was modified, multiple events may be received, we only care about data those modified data
                        #[cfg(unix)]
                        EventKind::Modify(notify::event::ModifyKind::Data(_)) => {
                            if let Ok(None) = handle_file_change(
                                moonc_opt,
                                moonbuild_opt,
                                registry_config,
                                module,
                                original_target_dir,
                                &event,
                            ) {
                                continue;
                            }
                        }
                        // windows has different file event kind
                        #[cfg(windows)]
                        EventKind::Modify(_) => {
                            if let Ok(None) = handle_file_change(
                                moonc_opt,
                                moonbuild_opt,
                                registry_config,
                                module,
                                original_target_dir,
                                &event,
                            ) {
                                continue;
                            }
                        }
                        _ => {
                            continue;
                        }
                    }
                }
                Err(e) => {
                    println!("failed: {:?}", e);
                    continue;
                }
            }
        }
    }
    Ok(0)
}

fn handle_file_change(
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    registry_config: &RegistryConfig,
    module: &ModuleDB,
    original_target_dir: &Path,
    event: &notify::Event,
) -> anyhow::Result<Option<()>> {
    // check --watch will own a subdir named `watch` in target_dir but build --watch still use the original target_dir
    let (source_dir, target_dir) = (&moonbuild_opt.source_dir, &moonbuild_opt.target_dir);
    let original_target_dir = match moonbuild_opt.run_mode {
        RunMode::Check => target_dir
            .ancestors()
            .find(|p| p.ends_with(WATCH_MODE_DIR))
            .unwrap()
            .parent()
            .unwrap(),
        _ => original_target_dir,
    };
    if event.paths.iter().all(|p| {
        p.starts_with(
            // can't be `target_dir` since the real target dir for watch mode is `target_dir/watch`
            original_target_dir,
        )
    }) {
        return Ok(None);
    }

    // prevent the case that the whole target_dir was deleted
    if !target_dir.exists() {
        std::fs::create_dir_all(target_dir).context(format!(
            "Failed to create target directory: '{}'",
            target_dir.display()
        ))?;
    }

    let mut need_new_module = false;
    let mut cur_mbt_md_path = String::new();
    for p in &event.paths {
        if p.display().to_string().ends_with(DOT_MBT_DOT_MD) {
            cur_mbt_md_path = p.display().to_string();
        }
        // we need to get the latest ModuleDB when moon.pkg.json || moon.mod.json is changed
        if p.ends_with(MOON_MOD_JSON) || p.ends_with(MOON_PKG_JSON) {
            need_new_module = true;
            break;
        }
    }

    if need_new_module {
        let (resolved_env, dir_sync_result) = match auto_sync(
            source_dir,
            &AutoSyncFlags { frozen: false },
            registry_config,
            false,
        ) {
            Ok((r, d)) => (r, d),
            Err(e) => {
                println!("failed at auto sync: {:?}", e);
                return Ok(None);
            }
        };
        let module = match moonutil::scan::scan(
            false,
            None,
            &resolved_env,
            &dir_sync_result,
            moonc_opt,
            moonbuild_opt,
        ) {
            Ok(m) => m,
            Err(e) => {
                println!("failed at scan: {:?}", e);
                return Ok(None);
            }
        };
        run_and_print(moonc_opt, moonbuild_opt, &module)?;
    } else {
        if cur_mbt_md_path.ends_with(DOT_MBT_DOT_MD) {
            for (_, pkg) in module.get_all_packages() {
                for (p, _) in &pkg.mbt_md_files {
                    if p.display().to_string() == cur_mbt_md_path {
                        let _ = moonutil::doc_test::gen_md_test_patch(pkg, moonc_opt)?;
                    }
                }
            }
        }
        run_and_print(moonc_opt, moonbuild_opt, module)?;
    }
    Ok(Some(()))
}

fn run_and_print(
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    module: &ModuleDB,
) -> anyhow::Result<()> {
    print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
    let result = match moonbuild_opt.run_mode {
        RunMode::Check => crate::entry::run_check(moonc_opt, moonbuild_opt, module),
        RunMode::Build => crate::entry::run_build(moonc_opt, moonbuild_opt, module),
        _ => {
            anyhow::bail!("watch mode only support check and build");
        }
    };
    match result {
        Ok(0) => {
            println!(
                "{}",
                "Success, waiting for filesystem changes...".green().bold()
            );
        }
        Err(e) => {
            println!(
                "{:?}\n{}",
                e,
                "Had errors, waiting for filesystem changes...".red().bold(),
            );
        }
        _ => {
            println!(
                "{}",
                "Had errors, waiting for filesystem changes...".red().bold(),
            );
        }
    }
    Ok(())
}
