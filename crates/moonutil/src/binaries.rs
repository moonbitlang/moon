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

use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::LazyLock;

fn ensure_exe_extension(path: PathBuf) -> PathBuf {
    #[cfg(target_os = "windows")]
    if path.extension().is_none() {
        return path.with_extension("exe");
    }
    path
}

fn resolve_executable_override(path: &OsStr) -> PathBuf {
    crate::toolchain::resolve_executable(path).unwrap_or_else(|_| {
        // Keep unresolved overrides intact so each command preserves its
        // existing user-facing error path.
        PathBuf::from(path)
    })
}

const EXECUTABLE_OVERRIDES: &[&str] = &[
    "MOON_OVERRIDE",
    "MOONC_OVERRIDE",
    "MOONCAKE_OVERRIDE",
    "MOON_IDE_OVERRIDE",
    "MOONDOC_OVERRIDE",
    "MOONFMT_OVERRIDE",
    "MOONINFO_OVERRIDE",
    "MOONRUN_OVERRIDE",
    "MOON_CRAM_OVERRIDE",
    "MOON_NODE_OVERRIDE",
];

const PAYLOAD_OVERRIDES: &[&str] = &["MOONLEX_OVERRIDE", "MOONYACC_OVERRIDE"];

/// Return only explicitly configured binary paths for best-effort display
/// redaction without initializing the cached binary inventory.
pub(crate) fn configured_binary_overrides() -> Vec<(&'static str, PathBuf)> {
    EXECUTABLE_OVERRIDES
        .iter()
        .filter_map(|&env_var| {
            std::env::var_os(env_var)
                .filter(|path| !path.is_empty())
                .map(|path| (env_var, resolve_executable_override(path.as_os_str())))
        })
        .chain(PAYLOAD_OVERRIDES.iter().filter_map(|&env_var| {
            std::env::var_os(env_var)
                .filter(|path| !path.is_empty())
                .map(|path| (env_var, PathBuf::from(path)))
        }))
        .collect()
}

fn moon_executable(binary_name: &str, env_var: Option<&str>) -> PathBuf {
    let current_dir =
        std::env::current_dir().expect("failed to get current directory for executable resolution");
    moon_executable_in(binary_name, env_var, &current_dir)
}

fn moon_executable_in(
    binary_name: &str,
    env_var: Option<&str>,
    current_dir: &std::path::Path,
) -> PathBuf {
    if let Some(env_var) = env_var
        && let Some(path) = std::env::var_os(env_var)
    {
        return crate::toolchain::resolve_executable_in(&path, current_dir).unwrap_or_else(|_| {
            // Keep unresolved overrides intact so each command preserves its
            // existing user-facing error path.
            PathBuf::from(path)
        });
    }

    if binary_name == "moon"
        && let Ok(current_exe) = std::env::current_exe()
        && current_exe
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "moon" || name == "moon.exe")
    {
        return dunce::canonicalize(&current_exe).unwrap_or(current_exe);
    }

    // Try to find in the resolved toolchain root.
    let in_toolchain = ensure_exe_extension(crate::moon_dir::bin().join(binary_name));
    if in_toolchain.exists() {
        return crate::toolchain::resolve_executable_in(&in_toolchain, current_dir).unwrap_or_else(
            |error| {
                panic!(
                    "failed to resolve MoonBit tool `{}`: {error:#}",
                    in_toolchain.display()
                )
            },
        );
    }

    if let Ok(in_path) = crate::toolchain::resolve_executable_in(binary_name, current_dir) {
        return in_path;
    }

    match env_var {
        Some(env_var) => panic!(
            "failed to resolve MoonBit tool `{binary_name}`; looked in `{}` and PATH. \
             Install the MoonBit toolchain or set `{env_var}` to an explicit path.",
            in_toolchain.display()
        ),
        None => panic!(
            "failed to resolve MoonBit tool `{binary_name}`; looked in `{}` and PATH. \
             Install the MoonBit toolchain.",
            in_toolchain.display()
        ),
    }
}

pub fn moon_cram_in(current_dir: &std::path::Path) -> PathBuf {
    moon_executable_in("moon-cram", Some("MOON_CRAM_OVERRIDE"), current_dir)
}

pub fn mooncake_in(current_dir: &std::path::Path) -> PathBuf {
    moon_executable_in("mooncake", Some("MOONCAKE_OVERRIDE"), current_dir)
}

fn moon_payload(file_name: &str, env_var: &str) -> PathBuf {
    if let Some(path) = std::env::var_os(env_var) {
        return PathBuf::from(path);
    }

    let in_toolchain = crate::moon_dir::bin().join(file_name);
    if in_toolchain.exists() {
        return in_toolchain;
    }

    // Preserve PATH lookup for installations that expose these payloads there,
    // while keeping override values under ordinary file-path semantics.
    if let Ok(in_path) = crate::toolchain::resolve_executable(file_name) {
        return in_path;
    }

    panic!(
        "failed to resolve MoonBit tool payload `{file_name}`; looked in `{}` and PATH. \
         Install the MoonBit toolchain or set `{env_var}` to an explicit path.",
        in_toolchain.display()
    )
}

fn optional_executable(candidates: &[&str], env_var: &str) -> Option<PathBuf> {
    if let Some(custom_path) = std::env::var_os(env_var) {
        return Some(resolve_executable_override(&custom_path));
    }
    candidates
        .iter()
        .find_map(|name| crate::toolchain::resolve_executable(name).ok())
}

fn get_fallback_binary(name: &str) -> PathBuf {
    ensure_exe_extension(PathBuf::from(name))
}

/// Lazily resolved programs used by Moon commands.
///
/// Entries are intentionally independent and demand-driven. Never add an
/// aggregate API that enumerates this cache: touching every `LazyLock` would
/// resolve the complete executable inventory and could make display,
/// diagnostics, or tests fail because an unused program is unavailable.
pub struct CachedBinaries {
    pub moonbuild: LazyLock<PathBuf>,
    pub moonc: LazyLock<PathBuf>,
    pub mooncake: LazyLock<PathBuf>,
    pub moon_ide: LazyLock<PathBuf>,
    pub moondoc: LazyLock<PathBuf>,
    pub moonfmt: LazyLock<PathBuf>,
    pub mooninfo: LazyLock<PathBuf>,
    pub moonlex: LazyLock<PathBuf>,
    pub moonrun: LazyLock<PathBuf>,
    pub moonyacc: LazyLock<PathBuf>,
    pub moon_cram: LazyLock<PathBuf>,
    pub moon_cove_report: LazyLock<PathBuf>,
    pub moonx: LazyLock<PathBuf>,
    pub node: LazyLock<Option<PathBuf>>,
    pub python: LazyLock<Option<PathBuf>>,
    pub git: LazyLock<Option<PathBuf>>,
}

impl CachedBinaries {
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
    moonbuild: LazyLock::new(|| moon_executable("moon", Some("MOON_OVERRIDE"))),
    moonc: LazyLock::new(|| moon_executable("moonc", Some("MOONC_OVERRIDE"))),
    mooncake: LazyLock::new(|| moon_executable("mooncake", Some("MOONCAKE_OVERRIDE"))),
    moon_ide: LazyLock::new(|| moon_executable("moon-ide", Some("MOON_IDE_OVERRIDE"))),
    moondoc: LazyLock::new(|| moon_executable("moondoc", Some("MOONDOC_OVERRIDE"))),
    moonfmt: LazyLock::new(|| moon_executable("moonfmt", Some("MOONFMT_OVERRIDE"))),
    mooninfo: LazyLock::new(|| moon_executable("mooninfo", Some("MOONINFO_OVERRIDE"))),
    moonlex: LazyLock::new(|| moon_payload("moonlex.wasm", "MOONLEX_OVERRIDE")),
    moonrun: LazyLock::new(|| moon_executable("moonrun", Some("MOONRUN_OVERRIDE"))),
    moonyacc: LazyLock::new(|| moon_payload("moonyacc.wasm", "MOONYACC_OVERRIDE")),
    moon_cram: LazyLock::new(|| moon_executable("moon-cram", Some("MOON_CRAM_OVERRIDE"))),
    moon_cove_report: LazyLock::new(|| moon_executable("moon_cove_report", None)),
    moonx: LazyLock::new(|| moon_executable("moonx", None)),
    node: LazyLock::new(|| optional_executable(&["node.cmd", "node"], "MOON_NODE_OVERRIDE")),
    python: LazyLock::new(|| optional_executable(&["python", "python3"], "MOON_PYTHON_OVERRIDE")),
    git: LazyLock::new(|| optional_executable(&["git"], "MOON_GIT_OVERRIDE")),
};

#[cfg(test)]
mod tests {
    use super::moon_executable;

    #[test]
    #[should_panic(expected = "failed to resolve MoonBit tool")]
    fn unresolved_moon_executable_panics_instead_of_bare_fallback() {
        let binary_name = format!(
            "__missing_moonbit_tool_for_binary_resolution_test_{}__",
            std::process::id()
        );
        let env_var = format!(
            "__MISSING_MOONBIT_TOOL_OVERRIDE_FOR_BINARY_RESOLUTION_TEST_{}__",
            std::process::id()
        );
        moon_executable(&binary_name, Some(&env_var));
    }
}
