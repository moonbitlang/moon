use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use colored::Colorize;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    common::GeneratedTestDriver,
    path::{ImportComponent, PathComponent},
};

#[derive(Debug, Clone)]
pub struct Package {
    pub is_main: bool,
    pub need_link: bool,
    pub is_third_party: bool,
    pub root_path: PathBuf,
    pub root: PathComponent,
    pub rel: PathComponent,
    // *.mbt
    pub files: Vec<PathBuf>,
    //  *_test.mbt
    pub test_files: Vec<PathBuf>,
    //  *_bbtest.mbt
    pub bbtest_files: Vec<PathBuf>,
    pub files_contain_test_block: Vec<PathBuf>,
    pub imports: Vec<ImportComponent>,
    pub test_imports: Vec<ImportComponent>,
    pub bbtest_imports: Vec<ImportComponent>,
    pub generated_test_drivers: Vec<GeneratedTestDriver>,
    pub artifact: PathBuf,

    pub link: Option<Link>,
    pub warn_list: Option<String>,
    pub alert_list: Option<String>,
}

impl Package {
    pub fn full_name(&self) -> String {
        if self.rel.full_name().is_empty() {
            self.root.full_name()
        } else {
            format!("{}/{}", self.root.full_name(), self.rel.full_name())
        }
    }

    pub fn last_name(&self) -> &str {
        if self.rel.components.is_empty() {
            self.root.components.last().unwrap()
        } else {
            self.rel.components.last().unwrap()
        }
    }

    pub fn full_components(&self) -> PathComponent {
        let mut comps = self.root.components.clone();
        comps.extend(self.rel.components.iter().cloned());
        PathComponent { components: comps }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct PackageJSON {
    pub is_main: bool,
    pub is_third_party: bool,
    pub root: String,
    pub rel: String,
    pub files: Vec<String>,
    pub test_files: Vec<String>,
    pub deps: Vec<AliasJSON>,
    pub test_deps: Vec<AliasJSON>,
    pub artifact: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct AliasJSON {
    pub path: String,
    pub alias: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum PkgJSONImport {
    Map(IndexMap<String, Option<String>>),
    List(Vec<PkgJSONImportItem>),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum PkgJSONImportItem {
    String(String),
    Object { path: String, alias: String },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BoolOrLink {
    Bool(bool),
    Link(Link),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MoonPkgJSON {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "is-main")]
    #[serde(alias = "is_main")]
    #[serde(rename(serialize = "is-main"))]
    pub is_main: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub import: Option<PkgJSONImport>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "test-import")]
    #[serde(alias = "test_import")]
    pub test_import: Option<PkgJSONImport>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "bbtest-import")]
    #[serde(alias = "bbtest_import")]
    pub bbtest_import: Option<PkgJSONImport>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<BoolOrLink>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "warn-list")]
    #[serde(alias = "warn_list")]
    pub warn_list: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "alert-list")]
    #[serde(alias = "alert_list")]
    pub alert_list: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LinkConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exports: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "export-memory-name")]
    pub export_memory_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JsLinkConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exports: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<JsFormat>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum JsFormat {
    #[default]
    #[serde(rename = "esm")]
    ESM,
    #[serde(rename = "cjs")]
    CJS,
    #[serde(rename = "iife")]
    IIFE,
}

impl JsFormat {
    pub fn to_flag(&self) -> &'static str {
        match self {
            JsFormat::ESM => "esm",
            JsFormat::CJS => "cjs",
            JsFormat::IIFE => "iife",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Link {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wasm: Option<LinkConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "wasm-gc")]
    pub wasm_gc: Option<LinkConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub js: Option<JsLinkConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MoonPkg {
    pub name: Option<String>,
    pub is_main: bool,
    pub need_link: bool,
    pub imports: Vec<Import>,
    pub test_imports: Vec<Import>,
    pub bbtest_imports: Vec<Import>,

    pub link: Option<Link>,
    pub warn_list: Option<String>,
    pub alert_list: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Import {
    Simple(String),
    Alias { path: String, alias: String },
}

impl Import {
    pub fn get_path(&self) -> &str {
        match self {
            Self::Simple(v) => v,
            Self::Alias { path, alias: _ } => path,
        }
    }
}

pub fn convert_pkg_json_to_package(j: MoonPkgJSON) -> anyhow::Result<MoonPkg> {
    let get_imports = |source: Option<PkgJSONImport>| -> Vec<Import> {
        let mut imports = vec![];
        if let Some(im) = source {
            match im {
                PkgJSONImport::Map(m) => {
                    for (k, v) in m.into_iter() {
                        match &v {
                            None => imports.push(Import::Simple(k)),
                            Some(p) => {
                                if p.is_empty() {
                                    imports.push(Import::Simple(k));
                                } else {
                                    imports.push(Import::Alias {
                                        path: k,
                                        alias: v.unwrap(),
                                    })
                                }
                            }
                        }
                    }
                }
                PkgJSONImport::List(l) => {
                    for item in l.into_iter() {
                        match item {
                            PkgJSONImportItem::String(s) => imports.push(Import::Simple(s)),
                            PkgJSONImportItem::Object { path, alias } => {
                                if alias.is_empty() {
                                    imports.push(Import::Simple(path));
                                } else {
                                    imports.push(Import::Alias { path, alias })
                                }
                            }
                        }
                    }
                }
            }
        };
        imports
    };

    let imports = get_imports(j.import);
    let test_imports = get_imports(j.test_import);
    let bbtest_imports = get_imports(j.bbtest_import);

    let mut is_main = j.is_main.unwrap_or(false);
    if let Some(name) = &j.name {
        if name == "main" {
            is_main = true;
            eprintln!(
                "{}",
                "Warning: The `name` field in `moon.pkg.json` is now deprecated. For the main package, please use `\"is-main\": true` instead. Refer to the latest documentation at https://www.moonbitlang.com/docs/build-system-tutorial for more information.".yellow()
                    .bold()
            );
        }
    }
    let need_link = match &j.link {
        None => false,
        Some(BoolOrLink::Bool(b)) => *b,
        Some(BoolOrLink::Link(_)) => true,
    };

    // TODO: check on the fly
    let mut alias_dedup: HashSet<String> = HashSet::new();
    for item in imports.iter() {
        let alias = match item {
            Import::Simple(p) => {
                let alias = Path::new(p)
                    .file_stem()
                    .context(format!("failed to get alias of `{}`", p))?
                    .to_str()
                    .unwrap()
                    .to_string();
                alias
            }
            Import::Alias { path: _path, alias } => alias.clone(),
        };
        if alias_dedup.contains(&alias) {
            bail!("Duplicate alias `{}`", alias);
        } else {
            alias_dedup.insert(alias.clone());
        }
    }

    // TODO: check on the fly
    let mut alias_dedup: HashSet<String> = HashSet::new();
    for item in test_imports.iter() {
        let alias = match item {
            Import::Simple(p) => {
                let alias = Path::new(p)
                    .file_stem()
                    .context(format!("failed to get alias of `{}`", p))?
                    .to_str()
                    .unwrap()
                    .to_string();
                alias
            }
            Import::Alias { path: _path, alias } => alias.clone(),
        };
        if alias_dedup.contains(&alias) {
            bail!("Duplicate alias `{}`", alias);
        } else {
            alias_dedup.insert(alias.clone());
        }
    }

    // TODO: check on the fly
    let mut alias_dedup: HashSet<String> = HashSet::new();
    for item in bbtest_imports.iter() {
        let alias = match item {
            Import::Simple(p) => {
                let alias = Path::new(p)
                    .file_stem()
                    .context(format!("failed to get alias of `{}`", p))?
                    .to_str()
                    .unwrap()
                    .to_string();
                alias
            }
            Import::Alias { path: _path, alias } => alias.clone(),
        };
        if alias_dedup.contains(&alias) {
            bail!("Duplicate alias `{}`", alias);
        } else {
            alias_dedup.insert(alias.clone());
        }
    }

    let result = MoonPkg {
        name: None,
        is_main,
        need_link,
        imports,
        test_imports,
        bbtest_imports,
        link: match j.link {
            None => None,
            Some(BoolOrLink::Bool(_)) => None,
            Some(BoolOrLink::Link(l)) => Some(l),
        },
        warn_list: j.warn_list,
        alert_list: j.alert_list,
    };
    Ok(result)
}
