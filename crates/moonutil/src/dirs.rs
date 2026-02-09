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

use std::{
    collections::{HashSet, hash_map::DefaultHasher},
    fs::{self, File},
    hash::Hasher,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::common::{
    BUILD_DIR, IGNORE_DIRS, MOON_MOD_JSON, MooncOpt, RunMode, get_moon_version, get_moonc_version,
    is_moon_pkg,
};

const MOON_DB: &str = "moon.db";

#[derive(Debug, Error)]
pub enum PackageDirsError {
    #[error("not in a Moon project (no moon.mod.json found starting from {0} or its ancestors)")]
    NotInProject(PathBuf),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, clap::Parser, Serialize, Deserialize, Clone)]
pub struct SourceTargetDirs {
    // NOTE: This separates "working directory" vs "project root".
    //
    // - `-C` changes the process working directory early (like `cd DIR && moon ...`).
    // - `--manifest-path` pins the project root to a specific `moon.mod.json`
    //   without changing the working directory.
    /// Change to DIR before doing anything else (must appear before the subcommand). Relative paths in other options and arguments are interpreted relative to DIR. Example: `moon -C a run .` runs the same as invoking `moon run .` from within `a`.
    #[arg(short = 'C', value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// Path to `moon.mod.json` to use as the project manifest (does not change the working directory).
    #[arg(long = "manifest-path", global = true, value_name = "PATH")]
    pub manifest_path: Option<PathBuf>,

    /// The target directory. Defaults to `<project-root>/_build`.
    #[clap(long, global = true)]
    pub target_dir: Option<PathBuf>,
}

impl SourceTargetDirs {
    pub fn try_into_package_dirs(&self) -> Result<PackageDirs, PackageDirsError> {
        PackageDirs::try_from(self)
    }
}

pub struct PackageDirs {
    pub source_dir: PathBuf,
    pub target_dir: PathBuf,
}

pub fn check_moon_mod_exists(source_dir: &Path) -> bool {
    source_dir.join(MOON_MOD_JSON).exists()
}

pub fn find_ancestor_with_mod(source_dir: &Path) -> Option<PathBuf> {
    source_dir
        .ancestors()
        .find(|dir| check_moon_mod_exists(dir))
        .map(|p| p.to_path_buf())
}

fn get_src_dst_dir(matches: &SourceTargetDirs) -> Result<PackageDirs, PackageDirsError> {
    let project_root = if let Some(manifest_path) = &matches.manifest_path {
        let manifest_path = dunce::canonicalize(manifest_path)
            .with_context(|| {
                format!(
                    "failed to resolve manifest path `{}`",
                    manifest_path.display()
                )
            })
            .map_err(PackageDirsError::from)?;

        if manifest_path.is_dir() {
            return Err(anyhow::anyhow!(
                "`--manifest-path` must point to `{}` (got directory `{}`)",
                MOON_MOD_JSON,
                manifest_path.display()
            )
            .into());
        }

        let file_name = manifest_path.file_name().and_then(|s| s.to_str());
        if file_name != Some(MOON_MOD_JSON) {
            return Err(anyhow::anyhow!(
                "`--manifest-path` must point to `{}` (got `{}`)",
                MOON_MOD_JSON,
                manifest_path.display()
            )
            .into());
        }

        manifest_path
            .parent()
            .context("manifest path has no parent directory")
            .map_err(PackageDirsError::from)?
            .to_path_buf()
    } else {
        let start_dir = std::env::current_dir()
            .context("failed to get current directory")
            .map_err(PackageDirsError::from)?;
        let start_dir = dunce::canonicalize(start_dir)
            .context("failed to resolve current directory")
            .map_err(PackageDirsError::from)?;
        find_ancestor_with_mod(&start_dir)
            .ok_or_else(|| PackageDirsError::NotInProject(start_dir.clone()))?
    };

    let target_dir = matches
        .target_dir
        .clone()
        .unwrap_or_else(|| project_root.join(BUILD_DIR));
    if !target_dir.exists() {
        std::fs::create_dir_all(&target_dir)
            .context("failed to create target directory")
            .map_err(PackageDirsError::from)?;
    }
    let target_dir = dunce::canonicalize(target_dir)
        .context("failed to set target directory")
        .map_err(PackageDirsError::from)?;

    Ok(PackageDirs {
        source_dir: project_root,
        target_dir,
    })
}

impl TryFrom<&SourceTargetDirs> for PackageDirs {
    type Error = PackageDirsError;

    fn try_from(matches: &SourceTargetDirs) -> Result<Self, Self::Error> {
        get_src_dst_dir(matches)
    }
}

pub fn mk_arch_mode_dir(
    source_dir: &Path,
    target_dir: &Path,
    moonc_opt: &MooncOpt,
    mode: RunMode,
) -> anyhow::Result<PathBuf> {
    let arch_dir = target_dir.join(moonc_opt.link_opt.target_backend.to_dir_name());
    let arch_mode_dir = if moonc_opt.build_opt.debug_flag {
        arch_dir.join("debug")
    } else {
        arch_dir.join("release")
    };
    if !arch_mode_dir.exists() {
        std::fs::create_dir_all(&arch_mode_dir)
            .context(format!("failed to create directory {arch_mode_dir:?}"))?;
    }

    let arch_mode_dir = arch_mode_dir.join(mode.to_dir_name());
    if !arch_mode_dir.exists() {
        std::fs::create_dir_all(&arch_mode_dir).expect("Failed to create target directory");
    }

    // this lock is used to prevent race condition on moon.db
    let _lock = crate::common::FileLock::lock(&arch_mode_dir)?;
    if !has_moon_db(&arch_mode_dir) {
        create_moon_db(source_dir, &arch_mode_dir)?;
    } else if need_rebuild(source_dir, &arch_mode_dir) {
        recreate_moon_db(source_dir, &arch_mode_dir)?;
        clean_dir_in_target(&arch_mode_dir)?;
    }

    Ok(arch_mode_dir)
}

#[derive(Debug, Serialize, Deserialize)]
struct FileHash {
    path: String,
    hash: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Fingerprint {
    moon_version: String,
    moonc_version: String,
    moon_pkgs: Vec<FileHash>,
    mbt_files: HashSet<String>,
}

fn hash_file(path: &str) -> u64 {
    let mut file = File::open(path).unwrap();
    let mut hasher = DefaultHasher::new();
    let mut buffer = [0; 1024];

    loop {
        let count = file.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        hasher.write(&buffer[..count]);
    }
    hasher.finish()
}

fn _get_fingerprint(moon_files: &[String], pkg_files: &[String]) -> Fingerprint {
    let mut fingerprint = Fingerprint {
        moon_version: get_moon_version(),
        moonc_version: get_moonc_version().unwrap(),
        moon_pkgs: vec![],
        mbt_files: HashSet::new(),
    };
    for file in moon_files {
        fingerprint.mbt_files.insert(file.clone());
    }
    for file in pkg_files {
        fingerprint.moon_pkgs.push(FileHash {
            path: file.clone(),
            hash: hash_file(file),
        })
    }
    fingerprint
}

fn get_project_files(
    dir: &Path,
    pkg_files: &mut Vec<String>,
    mbt_files: &mut Vec<String>,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if IGNORE_DIRS.contains(&path.file_name().unwrap().to_str().unwrap()) {
                continue;
            }
            get_project_files(&path, pkg_files, mbt_files)?;
        } else if let Some(extension) = path.extension()
            && let Some(filename) = path.file_name()
        {
            if extension == "mbt" {
                mbt_files.push(path.display().to_string());
            } else if let Some(s) = filename.to_str()
                && is_moon_pkg(s)
            {
                pkg_files.push(path.display().to_string())
            }
        }
    }
    Ok(())
}

pub fn clean_dir_in_target(target_dir: &Path) -> anyhow::Result<()> {
    let d = target_dir;
    if d.exists() {
        for entry in std::fs::read_dir(d)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                fs::remove_file(path)?;
            } else if path.is_dir() {
                fs::remove_dir_all(path)?;
            }
        }
    }
    Ok(())
}

pub fn has_moon_db(target_dir: &Path) -> bool {
    let moon_db = target_dir.join(MOON_DB);
    moon_db.exists()
}

pub fn recreate_moon_db(source_dir: &Path, target_dir: &Path) -> anyhow::Result<()> {
    let moon_db = target_dir.join(MOON_DB);
    let _ = std::fs::remove_file(moon_db);
    create_moon_db(source_dir, target_dir)
}

fn get_fingerprint(source_dir: &Path) -> Fingerprint {
    let mut pkg_files = vec![];
    let mut mbt_files = vec![];
    get_project_files(source_dir, &mut pkg_files, &mut mbt_files).unwrap();
    _get_fingerprint(&mbt_files, &pkg_files)
}

pub fn create_moon_db(source_dir: &Path, target_dir: &Path) -> anyhow::Result<()> {
    let moon_db = target_dir.join(MOON_DB);
    let fp = std::fs::File::create(moon_db).context("failed to create `moon.db`")?;
    let mut writer = std::io::BufWriter::new(fp);
    let finger = get_fingerprint(source_dir);
    let data = bincode::serialize(&finger).unwrap();
    writer.write_all(&data)?;
    Ok(())
}

fn load_moon_db(target_dir: &Path) -> anyhow::Result<Fingerprint> {
    let moon_db = target_dir.join(MOON_DB);
    let fp = std::fs::File::open(moon_db)?;
    let mut reader = std::io::BufReader::new(fp);
    let mut buf = vec![];
    reader.read_to_end(&mut buf)?;
    let _ = reader.read(&mut buf)?;
    let finger: Fingerprint = bincode::deserialize(&buf)?;
    Ok(finger)
}

fn files_hash_equal(lhs: &[FileHash], rhs: &[FileHash]) -> bool {
    let st1 = lhs.iter().map(|f| f.hash).collect::<HashSet<u64>>();
    let st2 = rhs.iter().map(|f| f.hash).collect::<HashSet<u64>>();
    st1 == st2
}

pub fn need_rebuild(source_dir: &Path, target_dir: &Path) -> bool {
    let old_fingerprint = load_moon_db(target_dir);
    if old_fingerprint.is_err() {
        return true;
    }
    let old_fingerprint = old_fingerprint.unwrap();
    let cur_fingerprint = get_fingerprint(source_dir);

    !(old_fingerprint.moon_version == cur_fingerprint.moon_version
        && old_fingerprint.moonc_version == cur_fingerprint.moonc_version
        && old_fingerprint.mbt_files == cur_fingerprint.mbt_files
        && files_hash_equal(&old_fingerprint.moon_pkgs, &cur_fingerprint.moon_pkgs))
}
