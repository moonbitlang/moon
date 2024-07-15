use anyhow::{anyhow, Context};
use colored::*;
use mooncake::pkg::sync::auto_sync;
use moonutil::module::ModuleDB;
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::mooncakes::RegistryConfig;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};

use crate::{bail_moon_check_is_running, write_current_pid, MOON_PID_NAME};
use moonutil::common::{MoonbuildOpt, MooncOpt};
use std::fs::remove_file;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn watch_single_thread(
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    registry_config: &RegistryConfig,
    module: &ModuleDB,
) -> anyhow::Result<i32> {
    let (source_dir, target_dir) = (&moonbuild_opt.source_dir, &moonbuild_opt.target_dir);

    let pid_path = target_dir.join(MOON_PID_NAME);
    if pid_path.exists() {
        bail_moon_check_is_running(&pid_path)?;
    }
    write_current_pid(target_dir, &pid_path)?;
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
    let (tx, rx) = std::sync::mpsc::channel();
    let tx_for_exit = tx.clone();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;

    let exit_flag = Arc::new(AtomicBool::new(false));
    {
        let r = exit_flag.clone();
        ctrlc::set_handler(move || {
            let exit_signal = notify::Event::new(notify::EventKind::Other);
            r.store(true, Ordering::SeqCst);
            let _ = tx_for_exit.send(Ok(exit_signal));
        })
        .expect("Error setting Ctrl-C handler");
    }

    {
        // main thread
        let exit_flag = exit_flag.clone();
        watcher.watch(source_dir, RecursiveMode::Recursive)?;
        for res in rx {
            match res {
                Ok(event) => {
                    {
                        if event.kind == notify::EventKind::Other
                            && exit_flag.load(Ordering::SeqCst)
                        {
                            break;
                        }
                    }

                    let all_in_target = event.paths.iter().all(|p| p.starts_with(target_dir));
                    if all_in_target {
                        continue;
                    }

                    if !target_dir.exists() {
                        std::fs::create_dir_all(target_dir)
                            .context("failed to create target dir")?;
                    }

                    print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
                    // we need to get the latest ModuleDB when we call run_check in watch mode
                    let (resolved_env, dir_sync_result) = auto_sync(
                        source_dir,
                        &AutoSyncFlags { frozen: false },
                        registry_config,
                        false,
                    )?;
                    let module = &moonutil::scan::scan(
                        false,
                        &resolved_env,
                        &dir_sync_result,
                        moonc_opt,
                        moonbuild_opt,
                    )?;
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
                Err(e) => {
                    return Err(anyhow!(e));
                }
            }
        }
    }
    remove_file(&pid_path).unwrap_or_default();
    Ok(0)
}
