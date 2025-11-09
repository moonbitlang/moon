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

use std::path::PathBuf;
use std::sync::LazyLock;

fn ensure_exe_extension(path: PathBuf) -> PathBuf {
    #[cfg(target_os = "windows")]
    if path.extension().is_none() {
        return path.with_extension("exe");
    }
    path
}

fn moon_bin(binary_name: &str, env_var: &str) -> PathBuf {
    if let Some(path) = std::env::var_os(env_var) {
        return PathBuf::from(path);
    }
    ensure_exe_extension(crate::moon_dir::bin().join(binary_name))
}

fn which_bin(candidates: &[&str], env_var: &str) -> Option<PathBuf> {
    if let Some(custom_path) = std::env::var_os(env_var) {
        return Some(PathBuf::from(custom_path));
    }
    candidates.iter().find_map(|name| which::which(name).ok())
}

fn get_fallback_binary(name: &str) -> PathBuf {
    ensure_exe_extension(PathBuf::from(name))
}

pub struct CachedBinaries {
    pub moonbuild: LazyLock<PathBuf>,
    pub moonc: LazyLock<PathBuf>,
    pub mooncake: LazyLock<PathBuf>,
    pub moondoc: LazyLock<PathBuf>,
    pub moonfmt: LazyLock<PathBuf>,
    pub mooninfo: LazyLock<PathBuf>,
    pub moonlex: LazyLock<PathBuf>,
    pub moonrun: LazyLock<PathBuf>,
    pub moonyacc: LazyLock<PathBuf>,
    pub moon_cove_report: LazyLock<PathBuf>,
    pub node: LazyLock<Option<PathBuf>>,
    pub python: LazyLock<Option<PathBuf>>,
    pub git: LazyLock<Option<PathBuf>>,
}

impl CachedBinaries {
    pub fn all_moon_bins(&self) -> Vec<(&str, PathBuf)> {
        vec![
            ("moon", self.moonbuild.clone()),
            ("moonc", self.moonc.clone()),
            ("mooncake", self.mooncake.clone()),
            ("moondoc", self.moondoc.clone()),
            ("moonfmt", self.moonfmt.clone()),
            ("mooninfo", self.mooninfo.clone()),
            ("moonlex", self.moonlex.clone()),
            ("moonrun", self.moonrun.clone()),
            ("moonyacc", self.moonyacc.clone()),
            ("moon_cove_report", self.moon_cove_report.clone()),
        ]
    }

    pub fn node_or_default(&self) -> PathBuf {
        self.node
            .clone()
            .unwrap_or_else(|| get_fallback_binary("node"))
    }

    pub fn git_or_default(&self) -> PathBuf {
        self.git
            .clone()
            .unwrap_or_else(|| get_fallback_binary("git"))
    }
}

pub static BINARIES: CachedBinaries = CachedBinaries {
    moonbuild: LazyLock::new(|| moon_bin("moon", "MOON_OVERRIDE")),
    moonc: LazyLock::new(|| moon_bin("moonc", "MOONC_OVERRIDE")),
    mooncake: LazyLock::new(|| moon_bin("mooncake", "MOONCAKE_OVERRIDE")),
    moondoc: LazyLock::new(|| moon_bin("moondoc", "MOONDOC_OVERRIDE")),
    moonfmt: LazyLock::new(|| moon_bin("moonfmt", "MOONFMT_OVERRIDE")),
    mooninfo: LazyLock::new(|| moon_bin("mooninfo", "MOONINFO_OVERRIDE")),
    moonlex: LazyLock::new(|| moon_bin("moonlex.wasm", "MOONLEX_OVERRIDE")),
    moonrun: LazyLock::new(|| moon_bin("moonrun", "MOONRUN_OVERRIDE")),
    moonyacc: LazyLock::new(|| moon_bin("moonyacc.wasm", "MOONYACC_OVERRIDE")),
    moon_cove_report: LazyLock::new(|| moon_bin("moon_cove_report", "MOON_COVE_REPORT_OVERRIDE")),
    node: LazyLock::new(|| which_bin(&["node.cmd", "node"], "MOON_NODE_OVERRIDE")),
    python: LazyLock::new(|| which_bin(&["python", "python3"], "MOON_PYTHON_OVERRIDE")),
    git: LazyLock::new(|| which_bin(&["git"], "MOON_GIT_OVERRIDE")),
};
