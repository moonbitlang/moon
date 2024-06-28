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

    let input = super::gen::gen_bundle::gen_bundle(module, moonc_opt, moonbuild_opt)?;
    log::debug!("{:#?}", input);
    super::gen::gen_bundle::gen_n2_bundle_state(&input, target_dir, moonc_opt, moonbuild_opt)
}
