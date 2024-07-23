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

use moonutil::common::{MoonbuildOpt, MooncOpt};
use moonutil::module::{convert_mdb_to_json, ModuleDB, ModuleDBJSON};
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
