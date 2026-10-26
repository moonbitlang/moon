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

//! Registry account, saved credentials, and configuration types.

use std::{fs::File, io::Write, path::Path};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use url::Url;

pub use crate::mooncakes::{Credentials, RegistryConfig, validate_username};

/// A saved login and the registry API that may receive its token.
///
/// The flattened representation preserves existing `Credentials` readers.
#[derive(Serialize, Deserialize)]
pub struct RegistryCredentials {
    #[serde(flatten)]
    pub credentials: Credentials,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    registry_api: Option<String>,
}

/// Validate and normalize a registry API base URL for credential matching.
pub fn registry_api_url(api: &str) -> anyhow::Result<Url> {
    let mut url = Url::parse(api).context("invalid registry API address")?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("registry API address must be HTTP(S) without userinfo, query, or fragment");
    }
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(&path);
    Ok(url)
}

impl RegistryCredentials {
    /// Consume the complete identity contract written by a registry login client.
    /// This is local validation, not a request to validate the token online.
    pub fn confirmed_username(&self, api: &str) -> anyhow::Result<&str> {
        let username = self
            .credentials
            .username
            .as_deref()
            .filter(|name| !name.trim().is_empty());
        if self.registry_api.is_none() || username.is_none() {
            bail!("Saved registry credentials are incomplete; run `moon login` again");
        }
        if !self.matches_registry(api)? || self.credentials.token.trim().is_empty() {
            bail!("Saved registry credentials do not match this registry; run `moon login` again");
        }
        Ok(username.expect("checked above"))
    }

    /// Read a saved login, returning `None` only when the file does not exist.
    pub fn load(path: &Path) -> anyhow::Result<Option<Self>> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("failed to open registry credentials"),
        };
        // Deserialization errors may quote secret values from the input.
        serde_json_lenient::from_reader(file)
            .map(Some)
            .map_err(|_| anyhow::anyhow!("failed to parse registry credentials"))
    }

    /// Save a login with its API binding, without changing the existing token format.
    pub fn save(path: &Path, credentials: Credentials, api: &str) -> anyhow::Result<()> {
        let saved = Self {
            credentials,
            registry_api: Some(registry_api_url(api)?.to_string()),
        };
        let temp_path = path.with_extension("tmp");
        {
            let file = File::create(&temp_path)?;
            let mut writer = std::io::BufWriter::new(file);
            serde_json_lenient::to_writer_pretty(&mut writer, &saved)?;
            writer.flush()?;
        }
        std::fs::rename(temp_path, path)?;
        Ok(())
    }

    /// Whether this login belongs to the selected API base URL.
    pub fn matches_registry(&self, api: &str) -> anyhow::Result<bool> {
        // TODO: Remove the official-only fallback once unbound logins are retired.
        let saved_api = self
            .registry_api
            .as_deref()
            .unwrap_or("https://mooncakes.io");
        Ok(registry_api_url(saved_api)? == registry_api_url(api)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials() -> Credentials {
        Credentials {
            token: "test-only-token".into(),
            username: Some("alice".into()),
        }
    }

    #[test]
    fn private_identity_requires_the_complete_saved_login() {
        let complete: RegistryCredentials = serde_json_lenient::from_str(
            r#"{"token":"test-token","username":"alice","registry_api":"https://example.com"}"#,
        )
        .unwrap();
        assert_eq!(
            complete.confirmed_username("https://example.com/").unwrap(),
            "alice"
        );
        assert!(
            complete
                .confirmed_username("https://other.example")
                .is_err()
        );
        for json in [
            r#"{"token":"test-token","registry_api":"https://example.com"}"#,
            r#"{"token":"test-token","username":"alice"}"#,
            r#"{"token":"test-token","username":" ","registry_api":"https://example.com"}"#,
        ] {
            let saved: RegistryCredentials = serde_json_lenient::from_str(json).unwrap();
            let error = saved.confirmed_username("https://example.com").unwrap_err();
            assert!(error.to_string().contains("moon login"));
            assert!(!error.to_string().contains("test-token"));
        }
    }

    #[test]
    fn saved_credentials_remain_compatible_with_existing_readers() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("credentials.json");
        RegistryCredentials::save(&path, credentials(), "https://Registry.Example:443/prefix/")
            .unwrap();
        let json = std::fs::read_to_string(&path).unwrap();
        let old: Credentials = serde_json_lenient::from_str(&json).unwrap();
        assert_eq!(old.token, "test-only-token");
        assert_eq!(old.username.as_deref(), Some("alice"));
        let saved = RegistryCredentials::load(&path).unwrap().unwrap();
        assert_eq!(
            saved.registry_api.as_deref(),
            Some("https://registry.example/prefix")
        );
        assert!(
            saved
                .matches_registry("https://registry.example/prefix/")
                .unwrap()
        );
        assert!(!path.with_extension("tmp").exists());
        RegistryCredentials::save(&path, credentials(), "https://other.example").unwrap();
        let saved = RegistryCredentials::load(&path).unwrap().unwrap();
        assert!(saved.matches_registry("https://other.example").unwrap());
    }

    #[test]
    fn legacy_credentials_only_match_the_official_api() {
        let saved: RegistryCredentials =
            serde_json_lenient::from_str(r#"{"token":"test-only-token","username":"alice"}"#)
                .unwrap();
        assert!(saved.matches_registry("https://mooncakes.io:443/").unwrap());
        assert!(!saved.matches_registry("https://other.example").unwrap());
        assert!(!saved.matches_registry("http://mooncakes.io").unwrap());
        assert!(!saved.matches_registry("https://mooncakes.io:8443").unwrap());
        assert!(
            !saved
                .matches_registry("https://mooncakes.io/other")
                .unwrap()
        );
    }

    #[test]
    fn bound_credentials_require_the_same_scheme_port_and_base_path() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("credentials.json");
        RegistryCredentials::save(&path, credentials(), "http://127.0.0.1:8001/prefix/").unwrap();
        let saved = RegistryCredentials::load(&path).unwrap().unwrap();
        assert!(
            saved
                .matches_registry("http://127.0.0.1:8001/prefix")
                .unwrap()
        );
        assert!(
            !saved
                .matches_registry("http://127.0.0.1:8002/prefix")
                .unwrap()
        );
        assert!(
            !saved
                .matches_registry("https://127.0.0.1:8001/prefix")
                .unwrap()
        );
        assert!(
            !saved
                .matches_registry("http://127.0.0.1:8001/prefix-other")
                .unwrap()
        );
        assert!(!saved.matches_registry("https://mooncakes.io").unwrap());
    }

    #[test]
    fn invalid_api_binding_preserves_the_previous_file() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("credentials.json");
        std::fs::write(&path, "original").unwrap();
        for api in [
            "not a URL",
            "file:///tmp/registry",
            "https://user:pass@registry.example",
            "https://registry.example?x=1",
            "https://registry.example#fragment",
        ] {
            assert!(RegistryCredentials::save(&path, credentials(), api).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
            assert!(!path.with_extension("tmp").exists());
        }
    }

    #[test]
    fn missing_credentials_are_optional_but_malformed_credentials_are_errors() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("credentials.json");
        assert!(RegistryCredentials::load(&path).unwrap().is_none());
        std::fs::write(&path, r#"{"token":"test-secret","username":"#).unwrap();
        let error = RegistryCredentials::load(&path).err().unwrap();
        assert!(!format!("{error:#}").contains("test-secret"));
        std::fs::write(&path, r#"{"token":42}"#).unwrap();
        assert!(RegistryCredentials::load(&path).is_err());
    }
}
