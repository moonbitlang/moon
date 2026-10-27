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

//! Registry HTTP clients using credentials loaded by their owning registry.

use anyhow::{Context, bail};
use moonutil::registry::{RegistryCredentials, registry_api_url};
use reqwest::{
    Url,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
    redirect::Policy,
};

pub(super) fn client(
    api: &str,
    credentials: Option<&RegistryCredentials>,
) -> anyhow::Result<reqwest::blocking::Client> {
    let mut builder = reqwest::blocking::Client::builder()
        .user_agent(format!("mooncake/{}", env!("CARGO_PKG_VERSION")));
    if let Some(credentials) = credentials {
        let api = registry_api_url(api)?;
        if let Some(authorization) = registry_authorization(credentials, &api)? {
            let mut headers = HeaderMap::new();
            headers.insert(AUTHORIZATION, authorization);
            let api_path = format!("{}/api/", api.path().trim_end_matches('/'));
            builder = builder
                .default_headers(headers)
                .redirect(Policy::custom(move |attempt| {
                    // Never forward credentials outside this registry's API,
                    // including same-host CDN paths and HTTPS downgrades.
                    if attempt.url().origin() != api.origin()
                        || !attempt.url().path().starts_with(&api_path)
                    {
                        attempt.stop()
                    } else if attempt.previous().len() >= 10 {
                        attempt.error("too many registry API redirects")
                    } else {
                        attempt.follow()
                    }
                }));
        }
    }
    builder
        .build()
        .context("failed to create registry HTTP client")
}

pub(super) fn registry_authorization(
    credentials: &RegistryCredentials,
    api: &Url,
) -> anyhow::Result<Option<HeaderValue>> {
    if !credentials.matches_registry(api.as_str())?
        || credentials.credentials.token.trim().is_empty()
    {
        return Ok(None);
    }
    let loopback = api.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    });
    if api.scheme() != "https" && !loopback {
        bail!("registry API credentials require HTTPS except on loopback addresses");
    }
    let mut authorization =
        HeaderValue::from_str(&format!("Bearer {}", credentials.credentials.token))
            .context("invalid registry API token header")?;
    authorization.set_sensitive(true);
    Ok(Some(authorization))
}
