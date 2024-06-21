use anyhow::Context;

use moonutil::common::gen::{convert_mdb_to_json, ModuleDB, ModuleDBJSON};
use moonutil::common::{MoonbuildOpt, MooncOpt};
use n2::load::State;
use std::io::BufWriter;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

pub fn write_pkg_lst(module: &ModuleDB, target_dir: &Path) -> anyhow::Result<()> {
    let create_and_write =
        |pkg_json_path: PathBuf, new_module_db_content: &ModuleDBJSON| -> anyhow::Result<()> {
            let fp = std::fs::File::create(pkg_json_path).context(format!(
                "failed to create `{}/packages.json`",
                target_dir.display()
            ))?;
            let mut writer = BufWriter::new(fp);
            let data = serde_json_lenient::to_vec_pretty(new_module_db_content)
                .context("failed to serialize packages list")?;
            writer.write_all(&data)?;

            Ok(())
        };

    let mj = convert_mdb_to_json(module);
    let pkg_json = target_dir.join("packages.json");

    // if the file exist and the old content is the same as the new content in `module`, don't rewrite it
    // otherwise we create and write
    if pkg_json.exists() {
        match std::fs::File::open(&pkg_json) {
            Ok(old_pkg_json_file) => {
                let old_pkg_json = serde_json_lenient::from_reader::<_, ModuleDBJSON>(
                    std::io::BufReader::new(old_pkg_json_file),
                );
                match old_pkg_json {
                    Ok(old_pkg_json) if old_pkg_json == mj => {
                        log::debug!(
                            "content of {} is the same, skip writing",
                            pkg_json.display()
                        );
                        Ok(())
                    }
                    _ => {
                        log::debug!("content of {} change, rewriting", pkg_json.display());
                        create_and_write(pkg_json, &mj)
                    }
                }
            }
            Err(_) => create_and_write(pkg_json, &mj),
        }
    } else {
        log::debug!("{} don't exist, try to create it", pkg_json.display());
        create_and_write(pkg_json, &mj)
    }
}

pub fn load_moon_proj(
    module: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<State> {
    let target_dir = &moonbuild_opt.target_dir;

    log::debug!("module: {:#?}", module);
    let n2_input = super::super::gen::gen_check::gen_check(module, moonc_opt, moonbuild_opt)?;
    log::debug!("n2_input: {:#?}", n2_input);
    super::super::gen::gen_check::gen_n2_check_state(
        &n2_input,
        target_dir,
        moonc_opt,
        moonbuild_opt,
    )
}
