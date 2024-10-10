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

use moonbuild::entry::{run_moon_generate, MoonGenerateState};
use moonutil::{
    common::{MoonbuildOpt, MooncOpt},
    module::ModuleDB,
    mooncakes::{result::ResolvedEnv, DirSyncResult},
};

pub fn scan_with_pre_build(
    doc_mode: bool,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    resolved_env: &ResolvedEnv,
    dir_sync_result: &DirSyncResult,
) -> anyhow::Result<ModuleDB> {
    let module = moonutil::scan::scan(
        doc_mode,
        resolved_env,
        dir_sync_result,
        moonc_opt,
        moonbuild_opt,
    )?;
    run_pre_build(
        moonc_opt,
        moonbuild_opt,
        module,
        resolved_env,
        dir_sync_result,
    )
}

fn run_pre_build(
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    module: ModuleDB,
    resolved_env: &ResolvedEnv,
    dir_sync_result: &DirSyncResult,
) -> anyhow::Result<ModuleDB> {
    let gen_result = run_moon_generate(moonbuild_opt, &module)?;
    let module = if let MoonGenerateState::WorkDone = gen_result {
        moonutil::scan::scan(
            false,
            resolved_env,
            dir_sync_result,
            moonc_opt,
            moonbuild_opt,
        )?
    } else {
        module
    };
    Ok(module)
}
