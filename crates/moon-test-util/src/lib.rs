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

pub mod cmdtest;
pub mod stack_trace;
pub mod test_dir;

pub fn insert_path_redaction(
    redactions: &mut snapbox::Redactions,
    placeholder: &'static str,
    path: &std::path::Path,
) -> snapbox::assert::Result<()> {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

    #[cfg(windows)]
    {
        for spelling in [path, canonical.as_path()] {
            let spelling = spelling.to_string_lossy();
            let spelling = spelling.strip_prefix(r"\\?\").unwrap_or(&spelling);
            redactions.insert(placeholder, std::path::PathBuf::from(spelling))?;
            redactions.insert(
                placeholder,
                std::path::PathBuf::from(format!(r"\\?\{spelling}")),
            )?;
        }
        Ok(())
    }

    #[cfg(not(windows))]
    {
        redactions.insert(placeholder, path.to_path_buf())?;
        redactions.insert(placeholder, canonical)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn path_redaction_matches_a_symlink_and_its_target() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let link = dir.path().join("link");
        std::fs::write(&target, "").unwrap();
        symlink(&target, &link).unwrap();

        let mut redactions = snapbox::Redactions::new();
        super::insert_path_redaction(&mut redactions, "[FILE]", &link).unwrap();
        assert_eq!(
            redactions.redact(&format!(
                "{}\n{}",
                link.display(),
                std::fs::canonicalize(&link).unwrap().display()
            )),
            "[FILE]\n[FILE]"
        );
    }

    #[test]
    fn snapbox_redacts_the_longer_windows_path_spelling_first() {
        let mut redactions = snapbox::Redactions::new();
        redactions
            .insert("[ROOT]", std::path::PathBuf::from(r"C:\workspace\source"))
            .unwrap();
        redactions
            .insert(
                "[ROOT]",
                std::path::PathBuf::from(r"\\?\C:\workspace\source"),
            )
            .unwrap();

        assert_eq!(
            redactions.redact(r"\\?\C:\workspace\source\main.mbt"),
            r"[ROOT]\main.mbt"
        );
        assert_eq!(
            redactions.redact("//?/C:/workspace/source/main.mbt"),
            "[ROOT]/main.mbt"
        );
    }
}
