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
    io::Write,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

pub(crate) fn cache_package<I, P, B>(
    moon_home: &Path,
    manifest: serde_json::Value,
    files: I,
) -> (PathBuf, PathBuf)
where
    I: IntoIterator<Item = (P, B)>,
    P: AsRef<str>,
    B: AsRef<[u8]>,
{
    let name = manifest["name"].as_str().expect("fixture module name");
    let version = manifest["version"].as_str().expect("fixture version");
    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    archive
        .start_file("moon.mod.json", zip::write::FileOptions::default())
        .unwrap();
    archive
        .write_all(&serde_json::to_vec_pretty(&manifest).unwrap())
        .unwrap();
    for (path, contents) in files {
        archive
            .start_file(path.as_ref(), zip::write::FileOptions::default())
            .unwrap();
        archive.write_all(contents.as_ref()).unwrap();
    }
    let archive = archive.finish().unwrap().into_inner();

    let (username, package) = name.split_once('/').unwrap();
    let cache_path = moon_home
        .join("registry/cache")
        .join(username)
        .join(package)
        .join(format!("{version}.zip"));
    std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
    std::fs::write(&cache_path, &archive).unwrap();

    let index_path = moon_home
        .join("registry/index/user")
        .join(username)
        .join(format!("{package}.index"));
    std::fs::create_dir_all(index_path.parent().unwrap()).unwrap();
    std::fs::write(
        &index_path,
        format!(
            "{{\"name\":\"{name}\",\"version\":\"{version}\",\"checksum\":\"{:x}\"}}\n",
            Sha256::digest(&archive)
        ),
    )
    .unwrap();

    (cache_path, index_path)
}
