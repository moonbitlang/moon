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
use notify::event::{DataChange, ModifyKind};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use moonutil::common::{MoonbuildOpt, MooncOpt, MOON_MOD_JSON, MOON_PKG_JSON, WATCH_MODE_DIR};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn watch_single_thread(
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    registry_config: &RegistryConfig,
    module: &ModuleDB,
) -> anyhow::Result<i32> {
    let (source_dir, target_dir) = (&moonbuild_opt.source_dir, &moonbuild_opt.target_dir);

    run_check_and_print(moonc_opt, moonbuild_opt, module);

    let (tx, rx) = std::sync::mpsc::channel();
    let tx_for_exit = tx.clone();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;

    let exit_flag = Arc::new(AtomicBool::new(false));
    {
        let r = Arc::clone(&exit_flag);
        ctrlc::set_handler(move || {
            let exit_signal = notify::Event::new(notify::EventKind::Other);
            r.store(true, Ordering::SeqCst);
            let _ = tx_for_exit.send(Ok(exit_signal));
        })
        .expect("Error setting Ctrl-C handler");
    }

    {
        // main thread
        let exit_flag = Arc::clone(&exit_flag);
        watcher.watch(source_dir, RecursiveMode::Recursive)?;

        // in watch mode, moon is a long-running process that should handle errors as much as possible rather than throwing them up and then exiting.
        for res in rx {
            match res {
                Ok(event) => {
                    match event.kind {
                        // receive quit signal (ctrl+c)
                        EventKind::Other if exit_flag.load(Ordering::SeqCst) => {
                            break;
                        }
                        // when a file was modified, multiple events may be received, we only care about data content changed event
                        EventKind::Modify(ModifyKind::Data(DataChange::Content)) => {
                            let origin_target_dir = target_dir
                                .ancestors()
                                .find(|p| p.ends_with(WATCH_MODE_DIR))
                                .unwrap()
                                .parent()
                                .unwrap();
                            if event.paths.iter().all(|p| {
                                p.starts_with(
                                    // can't be `target_dir` since the real target dir for watch mode is `target_dir/watch`
                                    origin_target_dir,
                                )
                            }) {
                                continue;
                            }

                            // prevent the case that the whole target_dir was deleted
                            if !target_dir.exists() {
                                std::fs::create_dir_all(target_dir).context(format!(
                                    "Failed to create target directory: '{}'",
                                    target_dir.display()
                                ))?;
                            }

                            if event
                                .paths
                                .iter()
                                .any(|p| p.ends_with(MOON_MOD_JSON) || p.ends_with(MOON_PKG_JSON))
                            {
                                // we need to get the latest ModuleDB when moon.pkg.json || moon.mod.json is changed
                                let (resolved_env, dir_sync_result) = match auto_sync(
                                    source_dir,
                                    &AutoSyncFlags { frozen: false },
                                    registry_config,
                                    false,
                                ) {
                                    Ok((r, d)) => (r, d),
                                    Err(e) => {
                                        println!("failed at auto sync: {:?}", e);
                                        continue;
                                    }
                                };
                                let module = match moonutil::scan::scan(
                                    false,
                                    &resolved_env,
                                    &dir_sync_result,
                                    moonc_opt,
                                    moonbuild_opt,
                                ) {
                                    Ok(m) => m,
                                    Err(e) => {
                                        println!("failed at scan: {:?}", e);
                                        continue;
                                    }
                                };
                                run_check_and_print(moonc_opt, moonbuild_opt, &module);
                            } else {
                                run_check_and_print(moonc_opt, moonbuild_opt, module);
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

fn run_check_and_print(moonc_opt: &MooncOpt, moonbuild_opt: &MoonbuildOpt, module: &ModuleDB) {
    print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
    let result = crate::entry::run_check(moonc_opt, moonbuild_opt, module);
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
}
