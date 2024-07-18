// Copyright 2024 International Digital Economy Academy
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
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    io::BufRead,
    path::Path,
    rc::Rc,
};

use anyhow::bail;
use moonutil::module::{MoonMod, MoonModJSON};
use moonutil::mooncakes::ModuleName;
use semver::Version;

pub struct OnlineRegistry {
    index: std::path::PathBuf,
    url_base: String, // TODO: add download feature to registry interface
    #[allow(clippy::type_complexity)] // Isn't it still pretty clear?
    cache: RefCell<HashMap<ModuleName, Rc<BTreeMap<Version, Rc<MoonMod>>>>>,
}

impl OnlineRegistry {
    pub fn mooncakes_io() -> Self {
        OnlineRegistry {
            index: moonutil::moon_dir::index(),
            url_base: "https://moonbitlang-mooncakes.s3.us-west-2.amazonaws.com/user".to_string(),
            cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn flush_cache(&mut self) {
        self.cache.borrow_mut().clear();
    }

    fn index_file_of(&self, name: &ModuleName) -> std::path::PathBuf {
        self.index
            .join("user")
            .join(&name.username)
            .join(format!("{}.index", name.pkgname))
    }
}

impl super::Registry for OnlineRegistry {
    fn all_versions_of(
        &self,
        name: &ModuleName,
    ) -> anyhow::Result<Rc<BTreeMap<Version, Rc<MoonMod>>>> {
        // check cache
        if let Some(v) = self.cache.borrow().get(name) {
            return Ok(v.clone());
        }

        let index_file = self.index_file_of(name);
        log::debug!("Reading versions of {} from {}", name, index_file.display());
        let file = std::fs::File::open(index_file)?;
        let reader = std::io::BufReader::new(file);

        let lines = reader.lines();
        let mut res = BTreeMap::new();
        for line in lines {
            let line = line?;
            let module: MoonModJSON = match serde_json_lenient::from_str(&line) {
                Ok(m) => m,
                Err(e) => {
                    log::warn!("Error when reading index file of {}: {}", name, e);
                    continue;
                }
            };
            let module: MoonMod = match module.try_into() {
                Ok(m) => m,
                Err(e) => {
                    log::warn!("Error when reading index file of {}: {}", name, e);
                    continue;
                }
            };
            if let Some(v) = &module.version {
                res.insert(v.clone(), Rc::new(module));
            }
        }

        // put in cache
        let res = Rc::new(res);
        self.cache.borrow_mut().insert(name.clone(), res.clone());

        Ok(res)
    }

    fn install_to(
        &self,
        name: &ModuleName,
        version: &Version,
        to: &Path,
        quiet: bool,
    ) -> anyhow::Result<()> {
        self.install_to_impl(name, version, to, quiet)
    }
}

pub fn calc_sha2(p: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    use std::fs::File;
    use std::io::prelude::*;

    let mut file = File::open(p)?;

    let mut hasher = Sha256::new();
    let mut buffer = [0; 1024];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    // read hash digest and consume hasher
    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

impl OnlineRegistry {
    fn read_checksum_from_index_file(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<String> {
        let p = self.index_file_of(name);
        let file = std::fs::File::open(&p)?;
        let reader = std::io::BufReader::new(file);

        let lines = reader.lines().collect::<std::io::Result<Vec<String>>>()?;
        for line in lines.iter().rev() {
            let j: MoonModJSON = serde_json_lenient::from_str(line)?;
            if j.version.as_ref() == Some(version) {
                if let Some(checksum) = j.checksum {
                    return Ok(checksum);
                } else {
                    bail!(
                        "No checksum found for version {} in index file {:?}",
                        version,
                        p
                    );
                }
            }
        }
        bail!(
            "No description found for version {} in index file {:?}",
            version,
            p,
        );
    }

    fn download_or_using_cache(
        &self,
        name: &ModuleName,
        version: &Version,
        quiet: bool,
    ) -> anyhow::Result<bytes::Bytes> {
        let pkg_index = self.index_file_of(name);
        if !pkg_index.exists() {
            anyhow::bail!("Module {}@{} not found", name, version);
        }
        let cache_file = cache_of(name, version);
        let mut checksum_ok = false;
        if cache_file.exists() {
            let checksum = self.read_checksum_from_index_file(name, version)?;
            let current_checksum = calc_sha2(&cache_file);
            if current_checksum.is_ok() && current_checksum.unwrap() == checksum {
                checksum_ok = true;
            }
        }
        if checksum_ok {
            if !quiet {
                println!("Using cached {}@{}", name, version);
            }
            let data = std::fs::read(cache_file)?;
            return Ok(bytes::Bytes::from(data));
        }
        if !quiet {
            println!("Downloading {}", name);
        }
        let filepath = form_urlencoded::Serializer::new(String::new())
            .append_key_only(&format!("{}/{}/{}", name.username, name.pkgname, version))
            .finish();
        let url = format!("{}/{}.zip", self.url_base, filepath);
        let data = reqwest::blocking::get(url)?.error_for_status()?.bytes()?;
        std::fs::create_dir_all(cache_file.parent().unwrap())?;
        std::fs::write(cache_file, &data)?;
        Ok(data)
    }

    pub fn install_to_impl(
        &self,
        name: &ModuleName,
        version: &Version,
        pkg_install_dir: &Path,
        quiet: bool,
    ) -> anyhow::Result<()> {
        // ensure dir exists and is empty
        if !pkg_install_dir.exists() {
            std::fs::create_dir_all(pkg_install_dir).unwrap();
        } else {
            std::fs::remove_dir_all(pkg_install_dir).unwrap();
            std::fs::create_dir_all(pkg_install_dir).unwrap();
        }

        let data = self.download_or_using_cache(name, version, quiet)?;
        let cursor = std::io::Cursor::new(data);
        let mut zip = zip::ZipArchive::new(cursor)?;
        for i in 0..zip.len() {
            let mut file = zip.by_index(i)?;
            let outpath = pkg_install_dir.join(file.mangled_name());

            if file.is_dir() {
                std::fs::create_dir_all(&outpath)?;
            } else {
                if let Some(parent) = outpath.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut outfile = std::fs::File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
        }
        Ok(())
    }
}

fn cache_of(name: &ModuleName, version: &Version) -> std::path::PathBuf {
    let cache_dir = moonutil::moon_dir::cache();

    cache_dir
        .join(&name.username)
        .join(&name.pkgname)
        .join(format!("{}.zip", version))
}

#[test]
fn test_urlencode() {
    let s = form_urlencoded::Serializer::new(String::new())
        .append_key_only("0.1.2+3")
        .finish();
    assert_eq!(s, "0.1.2%2B3");
}
