//! Legacy metadata JSON (`package.json`) conversion for IDE & tools usage.

use moonutil::{module::ModuleDBJSON, package::PackageJSON};

use crate::discover::DiscoveredPackage;

/// Generate `package.json`, which is a metadata file shared by IDE plugins and
/// other tools.
pub fn gen_metadata_json() -> ModuleDBJSON {
    ModuleDBJSON {
        source_dir: todo!(),
        name: todo!(),
        packages: todo!(),
        deps: todo!(),
        backend: todo!(),
        opt_level: todo!(),
        source: todo!(),
    }
}

fn gen_package_json(pkg: &DiscoveredPackage, is_in_workspace: bool) -> PackageJSON {
    PackageJSON {
        is_main: pkg.raw.is_main,
        is_third_party: !is_in_workspace,
        root_path: pkg.root_path.to_string_lossy().into_owned(),
        root: pkg.fqn.module().to_string(),
        rel: pkg.fqn.package().to_string(),
        files: todo!(),
        wbtest_files: todo!(),
        test_files: todo!(),
        mbt_md_files: todo!(),
        deps: todo!(),
        wbtest_deps: todo!(),
        test_deps: todo!(),
        artifact: todo!(),
    }
}
