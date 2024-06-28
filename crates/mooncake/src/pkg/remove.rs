use anyhow::bail;
use std::{path::Path, rc::Rc};

use moonutil::{
    common::{read_module_desc_file_in_dir, write_module_json_to_file},
    module::convert_module_to_mod_json,
    mooncakes::{ModuleSource, RegistryConfig},
};

use crate::resolver::resolve_single_root_with_defaults;

pub fn remove(
    source_dir: &Path,
    target_dir: &Path,
    username: &str,
    pkgname: &str,
    _registry_config: &RegistryConfig,
) -> anyhow::Result<i32> {
    let _ = target_dir;
    let mut m = read_module_desc_file_in_dir(source_dir)?;
    let removed = m.deps.shift_remove(&format!("{}/{}", username, pkgname));
    if removed.is_none() {
        bail!(
            "the dependency `{}/{}` could not be found",
            username,
            pkgname,
        )
    }
    let m = Rc::new(m);
    let ms = ModuleSource::from_local_module(&m, source_dir).expect("Malformed module manifest");
    let registry = crate::registry::RegistryList::with_default_registry();
    let res = resolve_single_root_with_defaults(&registry, ms, m.clone())?;

    let dep_dir = crate::dep_dir::DepDir::of_source(source_dir);
    crate::dep_dir::sync_deps(&dep_dir, &registry, &res, false)?;

    drop(res);

    let new_j = convert_module_to_mod_json(Rc::into_inner(m).unwrap());
    write_module_json_to_file(&new_j, source_dir)?;
    Ok(0)
}
