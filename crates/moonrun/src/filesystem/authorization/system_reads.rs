// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Built-in read grants for installed system data. Keep the inventory in
//! docs/dev/system-access.md in sync with these paths.
use super::{FsOperation, FsPath, FsTarget, GrantSource};
use std::path::{Component, Path, PathBuf};

const LOCALTIME: &str = "/etc/localtime";
const ZONEINFO: &str = "/usr/share/zoneinfo";

pub(super) fn allows(operation: &FsOperation<'_>, resolved: &Path) -> bool {
    let target = match *operation {
        FsOperation::Read(path)
        | FsOperation::AccessCheck { path, access: 0 | 1 } // F_OK or R_OK
        | FsOperation::OpenAsync {
            path,
            access: 0,
            create_mode: 0,
            ..
        } => FsTarget::Path(path),
        FsOperation::OpenWithOptions { path, options }
            if options.read
                && !(options.write
                    || options.create
                    || options.create_new
                    || options.truncate
                    || options.append) =>
        {
            FsTarget::Path(path)
        }
        FsOperation::Kind(target) | FsOperation::Metadata(target) => target,
        _ => return false,
    };
    // Do not follow an alias outside the trusted installation, or a relative
    // path whose meaning could change between authorization and a worker open.
    let requested = match target {
        FsTarget::Path(
            FsPath::Cwd(path) | FsPath::At { path, .. } | FsPath::AtHandle { path, .. },
        ) => Path::new(path),
        FsTarget::File(file) if let GrantSource::Path(path) = &file.0 => path,
        _ => return false,
    };
    timezone_file(requested, resolved)
}

fn timezone_file(requested: &Path, resolved: &Path) -> bool {
    !requested.components().any(|c| c == Component::ParentDir)
        && listed_path(requested)
        && listed_path(resolved)
        && resolved.is_file()
        && stays_in_installation(requested, resolved, listed_path)
}

// A trusted name could link through a writable directory and then back into
// zoneinfo. Its intermediate symlink could be retargeted after authorization.
// Expand one link at a time; every resulting full pathname must also be listed.
fn stays_in_installation(path: &Path, resolved: &Path, listed: impl Fn(&Path) -> bool) -> bool {
    let mut path = path.to_owned();
    for _ in 0..=40 {
        let mut prefix = PathBuf::new();
        let mut parts = path.components();
        let mut next = None;
        while let Some(part) = parts.next() {
            prefix.push(part);
            match std::fs::read_link(&prefix) {
                Ok(link) => {
                    prefix.pop();
                    // The prefix has already been walked without symlinks.
                    // Relative links in tzdata commonly begin with "../".
                    let mut link_parts = link.components();
                    while link_parts.clone().next() == Some(Component::ParentDir) {
                        prefix.pop();
                        link_parts.next();
                    }
                    let mut expanded = prefix.join(link_parts.as_path());
                    if !parts.as_path().as_os_str().is_empty() {
                        expanded.push(parts.as_path());
                    }
                    if expanded.components().any(|p| p == Component::ParentDir)
                        || !listed(&expanded)
                    {
                        return false;
                    }
                    next = Some(expanded);
                    break;
                }
                Err(e) if e.raw_os_error() == Some(libc::EINVAL) => {}
                Err(_) => return false,
            }
        }
        let Some(expanded) = next else {
            return prefix == resolved;
        };
        path = expanded;
    }
    false
}

fn listed_path(path: &Path) -> bool {
    if path == Path::new(LOCALTIME) || path.starts_with(ZONEINFO) {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        // /etc and /var have /private aliases. Apple's zoneinfo tree resolves
        // into /private/var/db/timezone/tz/<version>/zoneinfo. Naming only the
        // zoneinfo subtree excludes its sibling databases and unrelated files.
        if path == Path::new("/private/etc/localtime")
            || path.starts_with("/usr/share/zoneinfo.default")
        {
            return true;
        }
        let Ok(relative) = path
            .strip_prefix("/private")
            .or_else(|_| path.strip_prefix("/"))
        else {
            return false;
        };
        if relative.starts_with("var/db/timezone/zoneinfo") {
            return true;
        }
        if let Ok(versioned) = relative.strip_prefix("var/db/timezone/tz") {
            let mut parts = versioned.components();
            return matches!(parts.next(), Some(Component::Normal(_)))
                && parts.as_path().starts_with("zoneinfo");
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listed_name_cannot_grant_an_unlisted_symlink_target() {
        let private_file = tempfile::NamedTempFile::new().unwrap();
        for alias in [
            Path::new("/etc/localtime"),
            Path::new("/usr/share/zoneinfo/alias"),
        ] {
            assert!(!timezone_file(alias, private_file.path()));
        }
        assert!(!listed_path(Path::new(
            "/usr/share/zoneinfo-backup/private"
        )));
    }

    #[test]
    fn a_symlink_chain_must_stay_in_the_installation() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let database = root.join("zoneinfo");
        std::fs::create_dir_all(database.join("Region")).unwrap();
        let utc = database.join("UTC");
        std::fs::write(&utc, b"TZif").unwrap();
        let alias = database.join("Region/Alias");
        symlink("../UTC", &alias).unwrap();
        let listed = |path: &Path| path.starts_with(&database);
        assert!(stays_in_installation(&alias, &utc, listed));
        let outside = root.join("guest-alias");
        symlink(&utc, &outside).unwrap();
        let unsafe_alias = database.join("Outside");
        symlink(&outside, &unsafe_alias).unwrap();
        assert_eq!(dunce::canonicalize(&unsafe_alias).unwrap(), utc);
        assert!(!stays_in_installation(&unsafe_alias, &utc, listed));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn versioned_apple_database_excludes_siblings() {
        assert!(listed_path(Path::new(
            "/private/var/db/timezone/tz/2026d.1.0/zoneinfo/UTC"
        )));
        assert!(!listed_path(Path::new(
            "/private/var/db/timezone/tz/2026d.1.0/icutz/private"
        )));
    }
}
