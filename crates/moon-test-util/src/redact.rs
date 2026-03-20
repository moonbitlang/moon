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

use moonutil::{BINARIES, compiler_flags::CC};

pub fn canonicalize_or_self(path: &Path) -> PathBuf {
    dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn moon_home() -> Option<PathBuf> {
    std::env::var_os("MOON_HOME")
        .map(PathBuf::from)
        .or_else(|| home::home_dir().map(|home| home.join(".moon")))
}

#[derive(Default)]
pub struct OutputRedactor {
    redactions: snapbox::Redactions,
    replacements: Vec<(&'static str, String)>,
}

impl OutputRedactor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn value(
        mut self,
        placeholder: &'static str,
        replacement: impl Into<String>,
        value: impl Into<snapbox::RedactedValue>,
    ) -> Self {
        self.redactions
            .insert(placeholder, value)
            .expect("valid redaction");
        self.replacements.push((placeholder, replacement.into()));
        self
    }

    pub fn path(
        self,
        placeholder: &'static str,
        replacement: impl Into<String>,
        path: impl AsRef<Path>,
    ) -> Self {
        self.value(
            placeholder,
            replacement,
            canonicalize_or_self(path.as_ref()),
        )
    }

    pub fn redact(&self, output: &str) -> String {
        let output = output.replace("\\\\", "\\");
        let output = snapbox::filter::normalize_lines(&output);
        let output = self.redactions.redact(&output);
        let output = self
            .replacements
            .iter()
            .fold(output, |output, (placeholder, replacement)| {
                output.replace(placeholder, replacement)
            });
        snapbox::filter::normalize_paths(&output)
    }
}

pub fn common_output_redactor(root: &Path) -> OutputRedactor {
    let cc = CC::default();
    let redactor = OutputRedactor::new()
        .path("[ROOT]", "$ROOT", root)
        .value("[CC]", cc.cc_name(), cc.cc_path.clone())
        .value("[AR]", cc.ar_name(), cc.ar_path.clone());

    let redactor = match moon_home() {
        Some(moon_home) => redactor.path("[MOON_HOME]", "$MOON_HOME", moon_home),
        None => redactor,
    };

    BINARIES
        .all_moon_bins()
        .into_iter()
        .fold(redactor, |redactor, (name, path)| {
            let path = match name {
                #[allow(deprecated)]
                "moon" | "moonrun" => snapbox::cmd::cargo_bin(name),
                _ => path,
            };

            redactor.path(binary_placeholder(name), name, path)
        })
}

fn binary_placeholder(name: &str) -> &'static str {
    match name {
        "moon" => "[MOON]",
        "moonc" => "[MOONC]",
        "mooncake" => "[MOONCAKE]",
        "moondoc" => "[MOONDOC]",
        "moonfmt" => "[MOONFMT]",
        "mooninfo" => "[MOONINFO]",
        "moonlex" => "[MOONLEX]",
        "moonrun" => "[MOONRUN]",
        "moonyacc" => "[MOONYACC]",
        "moon_cove_report" => "[MOON_COVE_REPORT]",
        "node" => "[NODE]",
        "git" => "[GIT]",
        _ => unreachable!("unexpected binary placeholder for {name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::OutputRedactor;

    #[test]
    fn output_redactor_matches_native_and_normalized_paths() {
        let dir = tempfile::tempdir().unwrap();
        let root = super::canonicalize_or_self(dir.path());
        let normalized = root.to_string_lossy().replace('\\', "/");
        let output = format!("native: {}\nnormalized: {normalized}", root.display());

        let redacted = OutputRedactor::new()
            .path("[ROOT]", "$ROOT", &root)
            .redact(&output);

        assert_eq!(redacted, "native: $ROOT\nnormalized: $ROOT");
    }
}
