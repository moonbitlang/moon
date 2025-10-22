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

use n2::load::State;

use moonutil::{
    common::{MoonbuildOpt, MooncOpt},
    module::ModuleDB,
};

pub fn load_moon_proj(
    module: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<State> {
    let target_dir = &moonbuild_opt.target_dir;
    if !target_dir.exists() {
        std::fs::create_dir_all(target_dir)?;
    }

    log::debug!("{:#?}", module);

    let input = super::r#gen::gen_bundle::gen_bundle(module, moonc_opt, moonbuild_opt)?;
    log::debug!("{:#?}", input);
    super::r#gen::gen_bundle::gen_n2_bundle_state(&input, target_dir, moonc_opt, moonbuild_opt)
}
