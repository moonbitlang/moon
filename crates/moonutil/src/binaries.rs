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
    // Check for override via environment variable
    if let Some(path) = std::env::var_os(env_var) {
        return PathBuf::from(path);
    }

    // Try to find in the resolved toolchain root.
    let in_toolchain = ensure_exe_extension(crate::moon_dir::bin().join(binary_name));
    if in_toolchain.exists() {
        return in_toolchain;
    }

    // Try to resolve from PATH. This gives graph inputs a stable absolute path
    // when we need to track tool binaries as dependencies.
    if let Ok(in_path) = which::which(binary_name) {
        return in_path;
    }

    panic!(
        "failed to resolve MoonBit tool `{binary_name}`; looked in `{}` and PATH. \
         Install the MoonBit toolchain or set `{env_var}` to an explicit path.",
        in_toolchain.display()
    )
}

fn moon_bin_or_fallback(binary_name: &str, env_var: &str) -> PathBuf {
    if let Some(path) = std::env::var_os(env_var) {
        return PathBuf::from(path);
    }

    let in_toolchain = ensure_exe_extension(crate::moon_dir::bin().join(binary_name));
    if in_toolchain.exists() {
        return in_toolchain;
    }

    if let Some(next_to_moon) = find_binary_next_to_current_exe(binary_name) {
        return next_to_moon;
    }

    if let Ok(in_path) = which::which(binary_name) {
        return in_path;
    }

    get_fallback_binary(binary_name)
}

fn find_binary_next_to_current_exe(binary_name: &str) -> Option<PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    let current_dir = current_exe.parent()?;
    let mut candidates = vec![current_dir.to_path_buf()];
    if current_dir.file_name().is_some_and(|name| name == "deps")
        && let Some(parent) = current_dir.parent()
    {
        candidates.push(parent.to_path_buf());
    }

    candidates.into_iter().find_map(|dir| {
        let candidate = ensure_exe_extension(dir.join(binary_name));
        candidate.exists().then_some(candidate)
    })
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
    pub moon_ide: LazyLock<PathBuf>,
    pub moondoc: LazyLock<PathBuf>,
    pub moonfmt: LazyLock<PathBuf>,
    pub mooninfo: LazyLock<PathBuf>,
    pub moonlex: LazyLock<PathBuf>,
    pub moon_native_runner: LazyLock<PathBuf>,
    pub moonrun: LazyLock<PathBuf>,
    pub moonyacc: LazyLock<PathBuf>,
    pub moon_cram: LazyLock<PathBuf>,
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
            ("moon-native-runner", self.moon_native_runner.clone()),
            ("moonrun", self.moonrun.clone()),
            ("moonyacc", self.moonyacc.clone()),
            ("moon_cove_report", self.moon_cove_report.clone()),
            ("node", self.node_or_default()),
            ("git", self.git_or_default()),
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
    moon_ide: LazyLock::new(|| moon_bin("moon-ide", "MOON_IDE_OVERRIDE")),
    moondoc: LazyLock::new(|| moon_bin("moondoc", "MOONDOC_OVERRIDE")),
    moonfmt: LazyLock::new(|| moon_bin("moonfmt", "MOONFMT_OVERRIDE")),
    mooninfo: LazyLock::new(|| moon_bin("mooninfo", "MOONINFO_OVERRIDE")),
    moonlex: LazyLock::new(|| moon_bin("moonlex.wasm", "MOONLEX_OVERRIDE")),
    moon_native_runner: LazyLock::new(|| {
        moon_bin_or_fallback("moon-native-runner", "MOON_NATIVE_RUNNER_OVERRIDE")
    }),
    moonrun: LazyLock::new(|| moon_bin("moonrun", "MOONRUN_OVERRIDE")),
    moonyacc: LazyLock::new(|| moon_bin("moonyacc.wasm", "MOONYACC_OVERRIDE")),
    moon_cram: LazyLock::new(|| moon_bin("moon-cram", "MOON_CRAM_OVERRIDE")),
    moon_cove_report: LazyLock::new(|| moon_bin("moon_cove_report", "MOON_COVE_REPORT_OVERRIDE")),
    node: LazyLock::new(|| which_bin(&["node.cmd", "node"], "MOON_NODE_OVERRIDE")),
    python: LazyLock::new(|| which_bin(&["python", "python3"], "MOON_PYTHON_OVERRIDE")),
    git: LazyLock::new(|| which_bin(&["git"], "MOON_GIT_OVERRIDE")),
};

#[cfg(test)]
mod tests {
    use super::moon_bin;

    #[test]
    #[should_panic(expected = "failed to resolve MoonBit tool")]
    fn unresolved_moon_bin_panics_instead_of_bare_fallback() {
        let binary_name = format!(
            "__missing_moonbit_tool_for_binary_resolution_test_{}__",
            std::process::id()
        );
        let env_var = format!(
            "__MISSING_MOONBIT_TOOL_OVERRIDE_FOR_BINARY_RESOLUTION_TEST_{}__",
            std::process::id()
        );
        moon_bin(&binary_name, &env_var);
    }
}
