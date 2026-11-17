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

//! Registry endpoints and configuration file compatibility.

use std::{fs::File, io::BufReader};

use serde::Deserialize;

#[derive(Debug)]
pub struct RegistryConfig {
    pub api: String,
    pub index: String,
    pub download: String,
    pub symbols: Option<String>,
    /// TODO: Remove after support for `registry`-format configurations is dropped.
    /// Old configurations use the registry endpoint for prebuilt wasm assets.
    pub legacy_asset_urls: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RegistryConfigFile {
    Split {
        api: String,
        index: String,
        download: String,
        #[serde(default)]
        symbols: Option<String>,
    },
    Legacy {
        registry: String,
        index: String,
        #[serde(default)]
        symbols: Option<String>,
    },
}

impl From<RegistryConfigFile> for RegistryConfig {
    fn from(config: RegistryConfigFile) -> Self {
        match config {
            RegistryConfigFile::Split {
                api,
                index,
                download,
                symbols,
            } => Self {
                api,
                index,
                download,
                symbols,
                legacy_asset_urls: false,
            },
            RegistryConfigFile::Legacy {
                registry,
                index,
                symbols,
            } => {
                let download = if registry.trim_end_matches('/') == "https://mooncakes.io" {
                    "https://download.mooncakes.io".to_owned()
                } else {
                    registry.clone()
                };
                Self {
                    api: registry.clone(),
                    index,
                    download,
                    symbols,
                    legacy_asset_urls: true,
                }
            }
        }
    }
}

impl RegistryConfig {
    pub fn new() -> Self {
        if let Ok(v) = std::env::var("MOONCAKES_REGISTRY") {
            RegistryConfig {
                index: format!("{v}/git/index"),
                symbols: Some(format!("{v}/symbols.zip")),
                download: v.clone(),
                api: v,
                legacy_asset_urls: true,
            }
        } else {
            RegistryConfig {
                api: "https://mooncakes.io".into(),
                index: "https://mooncakes.io/git/index".into(),
                download: "https://download.mooncakes.io".into(),
                symbols: None,
                legacy_asset_urls: false,
            }
        }
    }

    pub fn load() -> Self {
        let config_path = moonutil::MOON_HOME.config_path();
        if !config_path.exists() {
            return Self::new();
        }
        let file = File::open(config_path).unwrap();
        let reader = BufReader::new(file);
        let config: RegistryConfigFile = serde_json_lenient::from_reader(reader).unwrap();
        config.into()
    }
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod registry_config_tests {
    use super::RegistryConfigFile;

    #[test]
    fn split_registry_config_uses_independent_endpoints() {
        let config: RegistryConfigFile = serde_json_lenient::from_str(
            r#"{
                "api": "https://api.example",
                "index": "https://index.example",
                "download": "https://download.example"
            }"#,
        )
        .unwrap();
        let config: super::RegistryConfig = config.into();

        assert_eq!(config.api, "https://api.example");
        assert_eq!(config.index, "https://index.example");
        assert_eq!(config.download, "https://download.example");
        assert!(!config.legacy_asset_urls);
    }

    #[test]
    fn legacy_registry_config_preserves_its_endpoint_contract() {
        let config: RegistryConfigFile = serde_json_lenient::from_str(
            r#"{
                "registry": "https://registry.example",
                "index": "https://index.example",
                "symbols": "https://registry.example/symbols.zip"
            }"#,
        )
        .unwrap();
        let config: super::RegistryConfig = config.into();

        assert_eq!(config.api, "https://registry.example");
        assert_eq!(config.download, "https://registry.example");
        assert!(config.legacy_asset_urls);
        assert_eq!(
            config.symbols.as_deref(),
            Some("https://registry.example/symbols.zip")
        );
    }

    #[test]
    fn legacy_official_registry_keeps_its_download_service() {
        let config: RegistryConfigFile = serde_json_lenient::from_str(
            r#"{
                "registry": "https://mooncakes.io",
                "index": "https://mooncakes.io/git/index"
            }"#,
        )
        .unwrap();
        let config: super::RegistryConfig = config.into();

        assert_eq!(config.api, "https://mooncakes.io");
        assert_eq!(config.download, "https://download.mooncakes.io");
        assert!(config.legacy_asset_urls);
    }
}
