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

use std::path::{Path, PathBuf};

use crate::{
    binaries::configured_binary_overrides,
    moon_dir::{home, toolchain_root},
};

/// Best-effort privacy and stability normalization for dry-run paths.
pub struct PathNormalizer {
    project_root: Option<PathBuf>,
    override_aliases: Vec<(String, String)>,
    current_program_alias: Option<(String, String)>,
    show_toolchain_root: bool,
    toolchain_root: String,
    moon_home: String,
}

impl PathNormalizer {
    pub fn new(source_dir: &Path) -> Self {
        let override_aliases = configured_binary_overrides()
            .into_iter()
            .filter(|(_, path)| !path.as_os_str().is_empty())
            .map(|(env_var, path)| (Self::display_path(&path), format!("${env_var}")))
            .collect();
        let current_program_alias = std::env::current_exe().ok().and_then(|path| {
            let name = path.file_name()?.to_str()?;
            Some((
                Self::display_path(&path),
                name.strip_suffix(".exe").unwrap_or(name).to_owned(),
            ))
        });

        Self::from_paths(
            source_dir,
            toolchain_root(),
            home(),
            override_aliases,
            current_program_alias,
        )
    }

    fn from_paths(
        source_dir: &Path,
        toolchain_root: PathBuf,
        moon_home: PathBuf,
        override_aliases: Vec<(String, String)>,
        current_program_alias: Option<(String, String)>,
    ) -> Self {
        let show_toolchain_root = match (
            dunce::canonicalize(&toolchain_root),
            dunce::canonicalize(&moon_home),
        ) {
            (Ok(toolchain_root), Ok(moon_home)) => toolchain_root != moon_home,
            _ => toolchain_root != moon_home,
        };

        PathNormalizer {
            project_root: dunce::canonicalize(source_dir).ok(),
            override_aliases,
            current_program_alias,
            show_toolchain_root,
            toolchain_root: Self::display_path(&toolchain_root),
            moon_home: Self::display_path(&moon_home),
        }
    }

    pub fn normalize_command(&self, command: &str) -> String {
        let args = crate::shlex::split_native(command);
        let normalized_args = args
            .iter()
            .enumerate()
            .map(|(index, arg)| {
                if index == 0 {
                    self.normalize_command_program(arg)
                } else {
                    self.normalize_command_arg(arg)
                }
            })
            .collect::<Vec<_>>();
        crate::shlex::join_unix(normalized_args.iter().map(String::as_str))
    }

    pub fn normalize_command_program(&self, program: &str) -> String {
        let raw_program = program.replace('\\', "/");
        if let Some(alias) = Self::exact_alias(&self.override_aliases, &raw_program) {
            return alias.clone();
        }
        if let Some((path, alias)) = &self.current_program_alias
            && path == &raw_program
        {
            return alias.clone();
        }
        let raw_toolchain_root = self.toolchain_root.trim_end_matches('/');
        if !raw_toolchain_root.is_empty() {
            let raw_toolchain_bin = format!("{raw_toolchain_root}/bin/");
            if let Some(file_name) = raw_program
                .strip_prefix(&raw_toolchain_bin)
                .filter(|file_name| !file_name.is_empty() && !file_name.contains('/'))
            {
                return file_name
                    .strip_suffix(".exe")
                    .unwrap_or(file_name)
                    .to_owned();
            }
        }

        let program = self.normalize_command_arg(&raw_program);
        let toolchain_bin = if self.show_toolchain_root {
            "$MOON_TOOLCHAIN_ROOT/bin/"
        } else {
            "$MOON_HOME/bin/"
        };

        program
            .strip_prefix(toolchain_bin)
            .filter(|file_name| !file_name.is_empty() && !file_name.contains('/'))
            .unwrap_or(&program)
            .to_owned()
    }

    pub fn normalize_command_arg(&self, value: &str) -> String {
        let value = value.replace('\\', "/");
        let mut value = Self::replace_complete_tokens(value, &self.override_aliases);
        if let Some(alias) = &self.current_program_alias {
            value = Self::replace_complete_tokens(value, std::slice::from_ref(alias));
        }
        let value = if let Some(root) = &self.project_root {
            Self::replace_path_prefix(value, &Self::display_path(root), ".")
        } else {
            value
        };
        self.mask_roots(value)
    }

    pub fn normalize_path(&self, path: &str) -> String {
        let normalized = path.replace('\\', "/");
        if let Some(masked) = Self::exact_alias(&self.override_aliases, &normalized) {
            return self.mask_roots(masked.clone());
        }
        let path_obj = Path::new(path);
        if let Some(root) = &self.project_root
            && let Ok(stripped) = path_obj.strip_prefix(root)
        {
            return Self::relative_from_path(stripped);
        }
        self.mask_roots(normalized)
    }

    pub fn normalize_context_path(&self, path: &Path) -> String {
        let normalized_path = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.normalize_path(&normalized_path.to_string_lossy())
    }

    fn exact_alias<'a>(aliases: &'a [(String, String)], value: &str) -> Option<&'a String> {
        aliases
            .iter()
            .find_map(|(from, to)| (value == from).then_some(to))
    }

    fn mask_roots(&self, mut value: String) -> String {
        if self.show_toolchain_root {
            value = Self::replace_path_prefix(value, &self.toolchain_root, "$MOON_TOOLCHAIN_ROOT");
        }
        value = Self::replace_path_prefix(value, &self.moon_home, "$MOON_HOME");

        // Executable suffixes under the toolchain bin directory are host
        // details; native build outputs such as `main.exe` remain unchanged.
        let Some(without_exe) = value.strip_suffix(".exe") else {
            return value;
        };
        let is_toolchain_binary =
            ["$MOON_HOME/bin/", "$MOON_TOOLCHAIN_ROOT/bin/"]
                .iter()
                .any(|bin_dir| {
                    without_exe
                        .rsplit_once(bin_dir)
                        .is_some_and(|(_, file_name)| {
                            !file_name.is_empty() && !file_name.contains('/')
                        })
                });
        if is_toolchain_binary {
            without_exe.to_owned()
        } else {
            value
        }
    }

    fn replace_complete_tokens(mut value: String, aliases: &[(String, String)]) -> String {
        for (path, alias) in aliases {
            if path.is_empty() {
                continue;
            }
            let mut cursor = 0;
            while let Some(offset) = value[cursor..].find(path) {
                let start = cursor + offset;
                let end = start + path.len();
                let before = value[..start].chars().next_back();
                let after = value[end..].chars().next();
                if Self::is_shell_token_boundary(before) && Self::is_shell_token_boundary(after) {
                    value.replace_range(start..end, alias);
                    cursor = start + alias.len();
                } else {
                    cursor = end;
                }
            }
        }
        value
    }

    fn is_shell_token_boundary(character: Option<char>) -> bool {
        character.is_none_or(|character| {
            character.is_whitespace()
                || matches!(
                    character,
                    '\'' | '"' | '|' | '&' | ';' | '(' | ')' | '<' | '>'
                )
        })
    }

    fn replace_path_prefix(mut value: String, path: &str, replacement: &str) -> String {
        let path = path.trim_end_matches('/');
        if path.is_empty() {
            return value;
        }
        // Roots may appear inside path lists and path-bearing compiler flags.
        // Requiring both boundaries avoids treating ordinary text as a path.
        let mut cursor = 0;
        while let Some(offset) = value[cursor..].find(path) {
            let start = cursor + offset;
            let end = start + path.len();
            let before = &value[..start];
            let after = value[end..].chars().next();
            let leading_boundary = Self::is_shell_token_boundary(before.chars().next_back())
                || before.ends_with(['=', ':', ';', ','])
                || ["-I", "-L", "/I", "/Fo", "/Fe", "/Fd", "@"]
                    .iter()
                    .any(|prefix| before.ends_with(prefix));
            let trailing_boundary = Self::is_shell_token_boundary(after)
                || after.is_some_and(|character| matches!(character, '/' | ':' | ';' | ','));
            if leading_boundary && trailing_boundary {
                value.replace_range(start..end, replacement);
                cursor = start + replacement.len();
            } else {
                cursor = end;
            }
        }
        value
    }

    fn display_path(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    fn relative_from_path(stripped: &Path) -> String {
        if stripped.as_os_str().is_empty() {
            ".".to_owned()
        } else {
            format!("./{}", Self::display_path(stripped))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PathNormalizer;
    use std::path::{Path, PathBuf};

    fn normalizer(
        toolchain_root: &str,
        moon_home: &str,
        override_aliases: Vec<(String, String)>,
        current_program_alias: Option<(String, String)>,
    ) -> PathNormalizer {
        PathNormalizer::from_paths(
            Path::new("."),
            toolchain_root.into(),
            moon_home.into(),
            override_aliases,
            current_program_alias,
        )
    }

    #[test]
    fn normalizes_each_program_source_without_touching_other_paths() {
        let temp = tempfile::tempdir().unwrap();
        let toolchain_root = PathNormalizer::display_path(&temp.path().join("toolchain"));
        let moon_home = PathNormalizer::display_path(&temp.path().join("home"));
        let toolchain_program = format!("{toolchain_root}/bin/moonc.exe");
        let override_program = PathNormalizer::display_path(&temp.path().join("custom/node.exe"));
        let current_program = PathNormalizer::display_path(&temp.path().join("build/moon.exe"));
        let normalizer = normalizer(
            &toolchain_root,
            &moon_home,
            vec![(override_program.clone(), "$MOON_NODE_OVERRIDE".to_owned())],
            Some((current_program.clone(), "moon".to_owned())),
        );

        for (program, expected) in [
            (&override_program, "$MOON_NODE_OVERRIDE"),
            (&current_program, "moon"),
            (&toolchain_program, "moonc"),
        ] {
            assert_eq!(normalizer.normalize_command_program(program), expected);
        }
        assert_eq!(
            normalizer.normalize_command_program("/usr/bin/node.exe"),
            "/usr/bin/node.exe"
        );
        assert_eq!(
            normalizer.normalize_path("/usr/bin/node.exe"),
            "/usr/bin/node.exe"
        );
        assert_eq!(
            normalizer.normalize_path(&toolchain_program),
            "$MOON_TOOLCHAIN_ROOT/bin/moonc"
        );
        assert_eq!(
            normalizer.normalize_path("./_build/native/debug/build/main/main.exe"),
            "./_build/native/debug/build/main/main.exe"
        );
    }

    #[test]
    fn renders_environment_aliases_as_symbolic_literals() {
        let temp = tempfile::tempdir().unwrap();
        let toolchain_root = PathNormalizer::display_path(&temp.path().join("toolchain"));
        let moon_home = PathNormalizer::display_path(&temp.path().join("home"));
        let override_program =
            PathNormalizer::display_path(&temp.path().join("custom compiler/moonc"));
        let normalizer = normalizer(
            &toolchain_root,
            &moon_home,
            vec![(override_program.clone(), "$MOONC_OVERRIDE".to_owned())],
            None,
        );
        let stdlib = format!("{moon_home}/lib/core");
        let command = crate::shlex::join_native(
            [override_program.as_str(), "--std-path", stdlib.as_str()].into_iter(),
        );

        assert_eq!(
            normalizer.normalize_command(&command),
            "'$MOONC_OVERRIDE' --std-path '$MOON_HOME/lib/core'"
        );
        assert_eq!(
            normalizer.normalize_command_arg(&format!("exec {override_program} --version")),
            "exec $MOONC_OVERRIDE --version"
        );
        assert_eq!(
            normalizer.normalize_command_arg(&format!("--compiler={override_program}")),
            format!("--compiler={override_program}")
        );
    }

    #[test]
    fn normalizes_the_current_program_when_it_is_a_complete_shell_token() {
        let temp = tempfile::tempdir().unwrap();
        let toolchain_root = PathNormalizer::display_path(&temp.path().join("toolchain"));
        let moon_home = PathNormalizer::display_path(&temp.path().join("home"));
        let current_program = PathNormalizer::display_path(&temp.path().join("build/moon"));
        let normalizer = normalizer(
            &toolchain_root,
            &moon_home,
            vec![],
            Some((current_program.clone(), "moon".to_owned())),
        );
        let shell_command = format!("{current_program} tool embed -i input -o output");
        let command = crate::shlex::join_native(
            [
                current_program.as_str(),
                "tool",
                "exec",
                "--shell",
                shell_command.as_str(),
            ]
            .into_iter(),
        );

        assert_eq!(
            normalizer.normalize_command(&command),
            "moon tool exec --shell 'moon tool embed -i input -o output'"
        );
        assert_eq!(
            normalizer.normalize_command_arg(&format!("{current_program}-helper")),
            format!("{current_program}-helper")
        );
    }

    #[test]
    fn masks_only_complete_overrides_and_path_root_boundaries() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = PathNormalizer::display_path(&dunce::canonicalize(".").unwrap());
        let toolchain = PathNormalizer::display_path(&temp.path().join("toolchain"));
        let moon_home = PathNormalizer::display_path(&temp.path().join("home"));
        let moon_home_with_slash = format!("{moon_home}/");
        let normalizer = normalizer(
            &toolchain,
            &moon_home_with_slash,
            vec![("node".to_owned(), "$MOON_NODE_OVERRIDE".to_owned())],
            None,
        );

        assert_eq!(
            normalizer.normalize_command_arg("node"),
            "$MOON_NODE_OVERRIDE"
        );
        assert_eq!(
            normalizer.normalize_command_arg("./node_modules/data"),
            "./node_modules/data"
        );
        assert_eq!(
            normalizer.normalize_command_arg(&format!("--project={workspace}/lib")),
            "--project=./lib"
        );
        for root in [&workspace, &toolchain, &moon_home] {
            let sibling = format!("{root}-cache/data");
            assert_eq!(normalizer.normalize_command_arg(&sibling), sibling);
            let embedded = format!("prefix{root}/lib");
            assert_eq!(normalizer.normalize_command_arg(&embedded), embedded);
        }
        assert_eq!(
            normalizer.normalize_command_arg(&format!("--root={toolchain}/lib")),
            "--root=$MOON_TOOLCHAIN_ROOT/lib"
        );
        assert_eq!(
            normalizer.normalize_command_arg(&format!("-I{toolchain}/include")),
            "-I$MOON_TOOLCHAIN_ROOT/include"
        );
        assert_eq!(
            normalizer.normalize_command_arg(&format!("/Fo{workspace}/_build/main.o")),
            "/Fo./_build/main.o"
        );
        assert_eq!(
            normalizer
                .normalize_command_arg(&format!("-Wl,-rpath,{workspace}/_build/native/debug/test")),
            "-Wl,-rpath,./_build/native/debug/test"
        );
        assert_eq!(
            normalizer.normalize_command_arg(&format!("--home={moon_home}")),
            "--home=$MOON_HOME"
        );
        assert_eq!(
            normalizer.normalize_command_arg(&format!("--home={moon_home}/lib")),
            "--home=$MOON_HOME/lib"
        );
        assert_eq!(
            normalizer.normalize_command_arg(&format!("tool --root '{toolchain}/lib'")),
            "tool --root '$MOON_TOOLCHAIN_ROOT/lib'"
        );

        let separator = if cfg!(windows) { ';' } else { ':' };
        assert_eq!(
            normalizer.normalize_command_arg(&format!(
                "{workspace}{separator}{moon_home}{separator}{toolchain}{separator}/sdk"
            )),
            format!(".{separator}$MOON_HOME{separator}$MOON_TOOLCHAIN_ROOT{separator}/sdk")
        );
    }

    #[test]
    fn uses_moon_home_alias_when_roots_match() {
        let temp = tempfile::tempdir().unwrap();
        let moon_home = PathNormalizer::display_path(&temp.path().join(".moon"));
        let normalizer = normalizer(&moon_home, &moon_home, vec![], None);

        assert_eq!(
            normalizer.normalize_command_arg(&format!("{moon_home}/lib/core/prelude")),
            "$MOON_HOME/lib/core/prelude"
        );
        assert_eq!(
            normalizer.normalize_command_program(&format!("{moon_home}/bin/moonc.exe")),
            "moonc"
        );

        // Canonically equal roots may retain distinct lexical spellings when
        // the configured toolchain root reaches Moon home through a symlink.
        let toolchain_spelling = PathNormalizer::display_path(&temp.path().join("toolchain-link"));
        let normalizer = PathNormalizer {
            toolchain_root: toolchain_spelling.clone(),
            ..normalizer
        };
        assert_eq!(
            normalizer.normalize_command_program(&format!("{toolchain_spelling}/bin/moonc.exe")),
            "moonc"
        );
    }

    #[test]
    fn matches_windows_paths_at_shell_and_compiler_option_boundaries() {
        let normalizer = normalizer(
            "C:/Moon/toolchain",
            "C:/Moon/home",
            vec![(
                "C:/Tools/moonc.exe".to_owned(),
                "$MOONC_OVERRIDE".to_owned(),
            )],
            None,
        );

        assert_eq!(
            normalizer.normalize_command_arg(r#"& "C:\Tools\moonc.exe" check"#),
            r#"& "$MOONC_OVERRIDE" check"#
        );
        assert_eq!(
            normalizer.normalize_command_arg(r#"/LIBPATH:C:\Moon\toolchain\lib"#),
            "/LIBPATH:$MOON_TOOLCHAIN_ROOT/lib"
        );
        assert_eq!(
            normalizer.normalize_command_arg(r#"prefixC:\Moon\toolchain\lib"#),
            "prefixC:/Moon/toolchain/lib"
        );
    }

    #[test]
    fn normalizes_context_paths_relative_to_the_source_directory() {
        let source_dir = tempfile::tempdir().unwrap();
        let package_dir = source_dir.path().join("pkg");
        std::fs::create_dir(&package_dir).unwrap();
        let normalizer = PathNormalizer::from_paths(
            source_dir.path(),
            PathBuf::from("/tmp/toolchain"),
            PathBuf::from("/tmp/home"),
            vec![],
            None,
        );

        assert_eq!(normalizer.normalize_context_path(&package_dir), "./pkg");
        assert_eq!(normalizer.normalize_context_path(source_dir.path()), ".");
    }
}
