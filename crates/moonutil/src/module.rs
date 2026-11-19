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

use crate::dependency::SourceDependencyInfo;
use crate::module_name::ModuleName;
use crate::package::{SupportedTargetsConfig, resolve_supported_targets};
use crate::target::TargetBackend;
use indexmap::map::IndexMap;
use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt::Debug};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MoonMod {
    pub name: String,
    pub version: Option<Version>,
    /// Requested private visibility for published versions of this module.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_private"
    )]
    pub private: Option<bool>,
    pub deps: IndexMap<String, SourceDependencyInfo>,
    pub readme: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub description: Option<String>,

    pub compile_flags: Option<Vec<String>>,
    pub link_flags: Option<Vec<String>>,
    pub checksum: Option<String>,
    pub source: Option<String>,
    pub rule: Option<Vec<MoonModRule>>,

    /// Fields not covered by the info above, which should be left as-is.
    #[serde(flatten)]
    pub ext: serde_json_lenient::Value,

    pub warn_list: Option<String>,

    /// Deprecated. Use `.gitignore` or `.moonignore` to control which files
    /// are packaged instead. `.moonignore` overrides `.gitignore` in the same
    /// directory.
    pub include: Option<Vec<String>>,
    /// Deprecated. Use `.gitignore` or `.moonignore` to control which files
    /// are packaged instead. `.moonignore` overrides `.gitignore` in the same
    /// directory.
    pub exclude: Option<Vec<String>>,

    pub preferred_target: Option<TargetBackend>,
    pub supported_targets: Option<SupportedTargetsConfig>,

    pub scripts: Option<IndexMap<String, String>>,
    pub __moonbit_unstable_prebuild: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MoonModRule {
    pub name: String,
    pub command: String,
    #[serde(flatten)]
    #[schemars(skip)]
    pub extra: serde_json_lenient::Map<String, serde_json_lenient::Value>,
}

// TODO: Remove the single-object input when legacy module JSON support is retired.
fn deserialize_rules<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<MoonModRule>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Rules {
        Single(MoonModRule),
        Multiple(Vec<MoonModRule>),
    }
    Ok(
        Option::<Rules>::deserialize(deserializer)?.map(|rules| match rules {
            Rules::Single(rule) => vec![rule],
            Rules::Multiple(rules) => rules,
        }),
    )
}

/// Internal normalized module representation, without a stable JSON input contract.
/// User manifests use the legacy JSON names or the DSL. Deserialization accepts
/// legacy names through aliases; accepting other spellings is incidental.
// TODO: Remove legacy input aliases only if user-authored JSON support is retired.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(
    title = "Internal normalized module JSON",
    description = "Unstable internal representation; not a user manifest format or a compatibility promise"
)]
pub struct MoonModJSON {
    /// name of the module
    pub name: String,

    /// version of the module
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Optional boolean selecting private visibility when true. When published,
    /// a private module must be accessible only to authorized users.
    /// Omit this field or set it to false for public visibility.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_private"
    )]
    #[schemars(with = "bool")]
    pub private: Option<bool>,

    /// third-party dependencies of the module
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<std::collections::HashMap<String, SourceDependencyInfo>>")]
    #[serde(alias = "deps")]
    pub import: Option<IndexMap<String, SourceDependencyInfo>>,

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

    /// Rules that can be referenced by package-level `dev_build` entries.
    #[serde(
        default,
        deserialize_with = "deserialize_rules",
        skip_serializing_if = "Option::is_none"
    )]
    pub rule: Option<Vec<MoonModRule>>,

    /// Fields not covered by the info above, which should be left as-is.
    #[serde(flatten)]
    #[schemars(skip)]
    pub ext: serde_json_lenient::Value,

    /// Warn list setting of the module
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "warnings", alias = "warn-list")]
    pub warnings: Option<String>,

    /// Deprecated. Use `.gitignore` or `.moonignore` to control which files
    /// are packaged instead. `.moonignore` overrides `.gitignore` in the same
    /// directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,

    /// Deprecated. Use `.gitignore` or `.moonignore` to control which files
    /// are packaged instead. `.moonignore` overrides `.gitignore` in the same
    /// directory.
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
    #[serde(rename = "preferred_target", alias = "preferred-target")]
    pub preferred_target: Option<String>,

    /// Supported backend set for all packages in this module.
    ///
    /// This uses the same expression/list grammar as `moon.pkg.json`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "supported_targets", alias = "supported-targets")]
    pub supported_targets: Option<SupportedTargetsConfig>,

    /// **Experimental:** A relative path to the pre-build configuration script.
    ///
    /// The script may be JavaScript (`.js`, `.cjs`, `.mjs`), Python (`.py`), or
    /// MoonBit (`.mbtx`), executed with Node.js, Python, or Moonrun respectively.
    /// MoonBit scripts are compiled to Wasm. Each script receives the process
    /// environment plus `MOON_MOD` (the module manifest path) and `MOON_BUILD_DIR`
    /// (a writable prebuild output directory created by Moon).
    /// `MOON_HOST_OS` and `MOON_HOST_ARCH` describe the Moon process's platform.
    /// `MOON_BACKEND`, `MOON_PROFILE`, and `MOON_JOBS` describe the project build.
    /// The original JSON input shape is still supplied on stdin for compatibility,
    /// with `paths.out_dir` set to the same output directory. Scripts return build
    /// configuration JSON on stdout.
    /// Since this is experimental, the API may change at any time without warning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub __moonbit_unstable_prebuild: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ModuleValidationError {
    #[error("`name` should not be empty")]
    EmptyName,
    #[error("`source` should not contain invalid chars `{0:?}`")]
    InvalidSourceChars(Vec<char>),
    #[error("`source` not a subdirectory of the parent directory")]
    SourceNotSubdirectory,
    #[error("`version` bad format")]
    Version(#[from] semver::Error),
    #[error(transparent)]
    ModuleVersion(#[from] crate::module_name::ModuleVersionError),
    #[error("`preferred_target` is not a valid backend")]
    PreferredTarget(anyhow::Error),
    #[error("`supported_targets` bad format")]
    SupportedTargets(anyhow::Error),
    #[error("Duplicate rule name `{0}` found in module manifest.")]
    DuplicateRule(String),
}

impl MoonMod {
    /// Validate module semantics after either input format has decoded its fields.
    pub(crate) fn validated(self) -> Result<Self, ModuleValidationError> {
        if self.name.is_empty() {
            return Err(ModuleValidationError::EmptyName);
        }
        ModuleName::from(self.name.as_str()).validate_version(self.version.as_ref())?;
        for (name, dep) in &self.deps {
            ModuleName::from(name.as_str()).validate_version(dep.version())?;
        }
        if let Some(source) = &self.source {
            let invalid: Vec<_> = source
                .chars()
                .filter(|c| ['<', '>', ':', '"', '|', '?', '*'].contains(c))
                .collect();
            if !invalid.is_empty() {
                return Err(ModuleValidationError::InvalidSourceChars(invalid));
            }
            if source.starts_with('/') || source.starts_with('\\') {
                return Err(ModuleValidationError::SourceNotSubdirectory);
            }
        }
        resolve_supported_targets(self.supported_targets.as_ref())
            .map_err(ModuleValidationError::SupportedTargets)?;
        let mut names = HashSet::new();
        for rule in self.rule.iter().flatten() {
            if !names.insert(rule.name.as_str()) {
                return Err(ModuleValidationError::DuplicateRule(rule.name.clone()));
            }
        }
        Ok(Self {
            source: self
                .source
                .map(|s| if s.is_empty() { ".".into() } else { s }),
            ..self
        })
    }
}

impl TryFrom<MoonModJSON> for MoonMod {
    type Error = ModuleValidationError;
    fn try_from(j: MoonModJSON) -> Result<Self, Self::Error> {
        MoonMod {
            name: j.name,
            version: j.version.as_deref().map(Version::parse).transpose()?,
            private: j.private,
            deps: j.import.unwrap_or_default(),
            readme: j.readme,
            repository: j.repository,
            license: j.license,
            keywords: j.keywords,
            description: j.description,
            compile_flags: j.compile_flags,
            link_flags: j.link_flags,
            checksum: j.checksum,
            source: j.source,
            rule: j.rule,
            ext: j.ext,
            warn_list: j.warnings,
            include: j.include,
            exclude: j.exclude,
            scripts: j.scripts,
            preferred_target: j
                .preferred_target
                .as_deref()
                .map(TargetBackend::str_to_backend)
                .transpose()
                .map_err(ModuleValidationError::PreferredTarget)?,
            supported_targets: j.supported_targets,
            __moonbit_unstable_prebuild: j.__moonbit_unstable_prebuild,
        }
        .validated()
    }
}

pub fn convert_module_to_mod_json(m: MoonMod) -> MoonModJSON {
    MoonModJSON {
        name: m.name,
        version: m.version.map(|v| v.to_string()),
        private: m.private,
        import: Some(m.deps),
        readme: m.readme,
        repository: m.repository,
        license: m.license,
        keywords: m.keywords,
        description: m.description,

        compile_flags: m.compile_flags,
        link_flags: m.link_flags,
        checksum: m.checksum,
        source: m.source,
        rule: m.rule,
        ext: m.ext,

        warnings: m.warn_list,

        include: m.include,
        exclude: m.exclude,

        scripts: m.scripts,

        preferred_target: m.preferred_target.map(|x| x.to_flag().to_owned()),
        supported_targets: m.supported_targets,

        __moonbit_unstable_prebuild: m.__moonbit_unstable_prebuild,
    }
}

impl From<MoonMod> for MoonModJSON {
    fn from(val: MoonMod) -> Self {
        convert_module_to_mod_json(val)
    }
}

// Absence is allowed, but an explicit null must not disable publishing protection.
fn deserialize_private<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<bool>, D::Error> {
    bool::deserialize(deserializer).map(Some)
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
