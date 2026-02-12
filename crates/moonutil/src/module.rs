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

use crate::common::{MoonModJSONFormatErrorKind, NameError, TargetBackend};
use crate::dependency::{
    BinaryDependencyInfo, BinaryDependencyInfoJson, SourceDependencyInfo, SourceDependencyInfoJson,
};
use crate::package::PackageJSON;
use indexmap::map::IndexMap;
use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ModuleDBJSON {
    pub source_dir: String,
    pub name: String,
    pub packages: Vec<PackageJSON>,
    pub deps: Vec<String>,
    pub backend: String,
    pub opt_level: String,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MoonMod {
    pub name: String,
    pub version: Option<Version>,
    pub deps: IndexMap<String, SourceDependencyInfo>,
    pub bin_deps: Option<IndexMap<String, BinaryDependencyInfo>>,
    pub readme: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub description: Option<String>,

    pub compile_flags: Option<Vec<String>>,
    pub link_flags: Option<Vec<String>>,
    pub checksum: Option<String>,
    pub source: Option<String>,

    /// Fields not covered by the info above, which should be left as-is.
    #[serde(flatten)]
    pub ext: serde_json_lenient::Value,

    pub warn_list: Option<String>,
    pub alert_list: Option<String>,

    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,

    pub preferred_target: Option<TargetBackend>,

    pub scripts: Option<IndexMap<String, String>>,
    pub __moonbit_unstable_prebuild: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(
    title = "JSON schema for MoonBit moon.mod.json files",
    description = "A module of MoonBit lang"
)]
pub struct MoonModJSON {
    /// name of the module
    pub name: String,

    /// version of the module
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// third-party dependencies of the module
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<std::collections::HashMap<String, SourceDependencyInfoJson>>")]
    pub deps: Option<IndexMap<String, SourceDependencyInfoJson>>,

    /// third-party binary dependencies of the module
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<std::collections::HashMap<String, BinaryDependencyInfoJson>>")]
    pub bin_deps: Option<IndexMap<String, BinaryDependencyInfoJson>>,

    /// path to module's README file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readme: Option<String>,

    /// url to module's repository
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,

    /// license of this module
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    /// keywords of this module
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,

    /// description of this module
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// custom compile flags
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub compile_flags: Option<Vec<String>>,

    /// custom link flags
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub link_flags: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub checksum: Option<String>,

    /// source code directory of this module
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "root-dir")]
    pub source: Option<String>,

    /// Fields not covered by the info above, which should be left as-is.
    #[serde(flatten)]
    #[schemars(skip)]
    pub ext: serde_json_lenient::Value,

    /// Warn list setting of the module
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warn_list: Option<String>,

    /// Alert list setting of the module
    ///     
    /// Please use `warn-list` instead. For example, `"warn-list": "-deprecated"` or `"warn-list": "-alert_unsafe"`.
    #[deprecated = r#"Please use `warn-list` instead. For example, `"warn-list": "-deprecated"` or `"warn-list": "-alert_unsafe"`"#]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alert_list: Option<String>,

    /// Files to include when publishing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,

    /// Files to exclude when publishing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<Vec<String>>,

    /// Scripts related to the current module.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<std::collections::HashMap<String, String>>")]
    pub scripts: Option<IndexMap<String, String>>,

    /// The preferred target backend of this module.
    ///
    /// Toolchains are recommended to use this target as the default target
    /// when the user is not specifying or overriding in any other ways.
    /// However, this is merely a recommendation, and tools may deviate from
    /// this value at any time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_target: Option<String>,

    /// **Experimental:** A relative path to the pre-build configuration script.
    ///
    /// The script should be a **JavaScript or Python** file that is able to be
    /// executed with vanilla Node.JS or Python interpreter. Since this is
    /// experimental, the API may change at any time without warning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub __moonbit_unstable_prebuild: Option<String>,
}

impl TryFrom<MoonModJSON> for MoonMod {
    type Error = MoonModJSONFormatErrorKind;
    fn try_from(j: MoonModJSON) -> Result<Self, Self::Error> {
        if j.name.is_empty() {
            return Err(MoonModJSONFormatErrorKind::Name(NameError::EmptyName));
        }

        let version = match &j.version {
            None => None,
            Some(v) => {
                Some(Version::parse(v.as_str()).map_err(MoonModJSONFormatErrorKind::Version)?)
            }
        };

        let deps = match j.deps {
            None => IndexMap::new(),
            Some(d) => d.into_iter().map(|(k, v)| (k, v.into())).collect(),
        };

        let bin_deps = j
            .bin_deps
            .map(|d| d.into_iter().map(|(k, v)| (k, v.into())).collect());

        let source = j.source.map(|s| if s.is_empty() { ".".into() } else { s });
        let preferred_target = j
            .preferred_target
            .map(|x| TargetBackend::str_to_backend(&x))
            .transpose()
            .map_err(MoonModJSONFormatErrorKind::PreferredBackend)?;

        #[allow(deprecated)]
        Ok(MoonMod {
            name: j.name,
            version,
            deps,
            bin_deps,
            readme: j.readme,
            repository: j.repository,
            license: j.license,
            keywords: j.keywords,
            description: j.description,

            compile_flags: j.compile_flags,
            link_flags: j.link_flags,
            checksum: j.checksum,
            source,
            ext: j.ext,

            alert_list: j.alert_list,
            warn_list: j.warn_list,

            include: j.include,
            exclude: j.exclude,

            scripts: j.scripts,
            preferred_target,

            __moonbit_unstable_prebuild: j.__moonbit_unstable_prebuild,
        })
    }
}

#[allow(deprecated)]
pub fn convert_module_to_mod_json(m: MoonMod) -> MoonModJSON {
    MoonModJSON {
        name: m.name,
        version: m.version.map(|v| v.to_string()),
        deps: Some(m.deps.into_iter().map(|(k, v)| (k, v.into())).collect()),
        bin_deps: m
            .bin_deps
            .map(|d| d.into_iter().map(|(k, v)| (k, v.into())).collect()),
        readme: m.readme,
        repository: m.repository,
        license: m.license,
        keywords: m.keywords,
        description: m.description,

        compile_flags: m.compile_flags,
        link_flags: m.link_flags,
        checksum: m.checksum,
        source: m.source,
        ext: m.ext,

        alert_list: m.alert_list,
        warn_list: m.warn_list,

        include: m.include,
        exclude: m.exclude,

        scripts: m.scripts,

        preferred_target: m.preferred_target.map(|x| x.to_flag().to_owned()),

        __moonbit_unstable_prebuild: m.__moonbit_unstable_prebuild,
    }
}

impl From<MoonMod> for MoonModJSON {
    fn from(val: MoonMod) -> Self {
        convert_module_to_mod_json(val)
    }
}

#[test]
fn validate_mod_json_schema() {
    let schema = schemars::schema_for!(MoonModJSON);
    let actual = &serde_json_lenient::to_string_pretty(&schema).unwrap();
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../moonbuild/template/mod.schema.json"
    );
    expect_test::expect_file![path].assert_eq(actual);

    let html_template_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../moonbuild/template/mod_json_schema.html"
    );
    let html_template = std::fs::read_to_string(html_template_path).unwrap();
    let content = html_template.replace("const schema = {}", &format!("const schema = {actual}"));
    let html_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/manual/src/source/mod_json_schema.html"
    );
    std::fs::write(html_path, &content).unwrap();

    let html_path_zh = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/manual-zh/src/source/mod_json_schema.html"
    );
    std::fs::write(html_path_zh, content).unwrap();
}
