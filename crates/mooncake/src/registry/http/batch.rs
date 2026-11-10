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

//! Bounded exact-version batches; caches and authentication stay in HttpMetadata.

use super::{HttpVersionReader, ModuleMetadata};
use crate::registry::{RegistryUnavailable, RegistryVersionInfo};
use anyhow::{Context, bail};
use moonutil::{registry::registry_api_url, resolution::ModuleName};
use reqwest::StatusCode;
use semver::Version;
use serde::Deserialize;
use std::sync::Arc;

pub(super) const MAX_COORDINATES: usize = 64;
const MAX_REQUEST_BYTES: usize = 65_536;
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
type Coordinate = (ModuleName, Version);

#[derive(Deserialize)]
struct BatchResponse {
    results: Vec<BatchItem>,
}

#[derive(Deserialize)]
struct BatchItem {
    module: String,
    version: Version,
    status: u16,
    metadata: Option<ModuleMetadata>,
}

fn request_body(versions: &[Coordinate]) -> anyhow::Result<Vec<u8>> {
    Ok(serde_json::to_vec(&serde_json::json!({
        "modules": versions.iter().map(|(name, version)| {
            serde_json::json!({"module": name.to_string(), "version": version.to_string()})
        }).collect::<Vec<_>>()
    }))?)
}

/// Bound bytes actually read, including when Content-Length is absent or false.
async fn read_response(mut response: reqwest::Response) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| RegistryUnavailable(error.into()))?
    {
        if chunk.len() > MAX_RESPONSE_BYTES - bytes.len() {
            bail!("registry batch response exceeds the 4 MiB limit");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// Preserve each coordinate's error, including the unavailable classification.
fn repeat_error<T>(error: anyhow::Error, count: usize) -> Vec<anyhow::Result<T>> {
    let unavailable = error.is::<RegistryUnavailable>();
    let shared: Arc<dyn std::error::Error + Send + Sync> = error.into_boxed_dyn_error().into();
    (0..count)
        .map(|_| {
            let error = anyhow::Error::new(Arc::clone(&shared));
            Err(if unavailable {
                RegistryUnavailable(error).into()
            } else {
                error
            })
        })
        .collect()
}

impl HttpVersionReader {
    pub(super) async fn version_infos(
        &self,
        versions: &[Coordinate],
    ) -> Vec<anyhow::Result<RegistryVersionInfo>> {
        let mut results: Vec<Option<anyhow::Result<RegistryVersionInfo>>> =
            (0..versions.len()).map(|_| None).collect();
        let mut missing: indexmap::IndexMap<Coordinate, Vec<usize>> = indexmap::IndexMap::new();
        let state = Arc::clone(&self.state);
        let owned = versions.to_vec();
        let cached = match tokio::task::spawn_blocking(move || {
            owned
                .iter()
                .map(|(name, version)| {
                    super::super::path::parse_module_path(&name.to_string())?;
                    name.validate_version(Some(version))?;
                    state.cached_release(name, version)
                })
                .collect::<Vec<_>>()
        })
        .await
        {
            Ok(cached) => cached,
            Err(error) => return repeat_error(error.into(), versions.len()),
        };
        for (index, ((name, version), cached)) in versions.iter().zip(cached).enumerate() {
            match cached {
                Ok(Some(metadata)) => results[index] = Some(Ok(metadata.into())),
                Err(error) => results[index] = Some(Err(error)),
                Ok(None) if self.frozen => {
                    results[index] = Some(Err(anyhow::anyhow!(
                        "missing cached registry metadata for {name}; frozen mode cannot fetch it"
                    )));
                }
                Ok(None) => missing
                    .entry((name.clone(), version.clone()))
                    .or_default()
                    .push(index),
            }
        }
        let coordinates = missing.keys().cloned().collect::<Vec<_>>();
        for chunk in coordinates.chunks(MAX_COORDINATES) {
            let fetched = self
                .fetch_batch(chunk)
                .await
                .unwrap_or_else(|error| repeat_error(error, chunk.len()));
            let state = Arc::clone(&self.state);
            let owned = chunk.to_vec();
            let fetched = match tokio::task::spawn_blocking(move || {
                owned
                    .into_iter()
                    .zip(fetched)
                    .map(|((name, _), outcome)| {
                        outcome.and_then(|metadata| {
                            state
                                .cache
                                .store_release(&name, metadata)
                                .map(RegistryVersionInfo::from)
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .await
            {
                Ok(fetched) => fetched,
                Err(error) => repeat_error(error.into(), chunk.len()),
            };
            for ((name, version), outcome) in chunk.iter().zip(fetched) {
                let indices = &missing[&(name.clone(), version.clone())];
                let copies = match outcome {
                    Ok(info) => (0..indices.len()).map(|_| Ok(info.clone())).collect(),
                    Err(error) => repeat_error(error, indices.len()),
                };
                for (&index, result) in indices.iter().zip(copies) {
                    results[index] = Some(result);
                }
            }
        }
        results
            .into_iter()
            .map(|result| result.expect("one result per coordinate"))
            .collect()
    }

    async fn single_reads(&self, versions: &[Coordinate]) -> Vec<anyhow::Result<ModuleMetadata>> {
        let mut results = Vec::with_capacity(versions.len());
        for (name, version) in versions {
            results.push(self.request_release(name, version).await);
        }
        results
    }

    /// Every recursive call halves a nonempty slice; fallback stays on the same
    /// captured client identity and registry. Malformed/denied answers never retry.
    async fn fetch_batch(
        &self,
        versions: &[Coordinate],
    ) -> anyhow::Result<Vec<anyhow::Result<ModuleMetadata>>> {
        if versions.is_empty() {
            return Ok(Vec::new());
        }
        if !self.batch_supported.get() {
            return Ok(self.single_reads(versions).await);
        }
        let body = request_body(versions)?;
        if body.len() > MAX_REQUEST_BYTES {
            return self.split_batch(versions).await;
        }
        // A legacy GET may support names outside the batch protocol's limits.
        if versions.iter().any(|(name, version)| {
            name.to_string().encode_utf16().count() > 1024 || version.to_string().len() > 256
        }) {
            if versions.len() == 1 {
                return Ok(self.single_reads(versions).await);
            }
            return self.split_batch(versions).await;
        }
        let api = registry_api_url(&self.state.api)?;
        let url = format!(
            "{}/api/v0/registry/metadata",
            api.as_str().trim_end_matches('/')
        );
        let response = self
            .http_client()?
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(|error| RegistryUnavailable(error.into()))?;
        let status = response.status();
        if matches!(
            status,
            StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED | StatusCode::NOT_IMPLEMENTED
        ) {
            self.batch_supported.set(false);
            drop(response);
            return Ok(self.single_reads(versions).await);
        }
        if status == StatusCode::PAYLOAD_TOO_LARGE {
            drop(response);
            return self.split_batch(versions).await;
        }
        if status == StatusCode::UNPROCESSABLE_ENTITY {
            let bytes = read_response(response).await?;
            #[derive(Deserialize)]
            struct BatchError {
                code: String,
            }
            if serde_json::from_slice::<BatchError>(&bytes)
                .is_ok_and(|error| error.code == "batch_response_too_large")
            {
                return self.split_batch(versions).await;
            }
            bail!("registry rejected batch metadata request (HTTP 422)");
        }
        // Keep public/private denials opaque and never treat them as unsupported.
        if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
            bail!("registry batch metadata not found or access denied");
        }
        let response = response.error_for_status().map_err(|error| {
            if status.is_server_error()
                || matches!(
                    status,
                    StatusCode::REQUEST_TIMEOUT | StatusCode::TOO_MANY_REQUESTS
                )
            {
                anyhow::Error::new(RegistryUnavailable(error.into()))
            } else {
                error.into()
            }
        })?;
        let bytes = read_response(response).await?;
        let batch: BatchResponse =
            serde_json::from_slice(&bytes).context("invalid registry batch response")?;
        if batch.results.len() != versions.len() {
            bail!("registry batch response has a different number of results");
        }
        // Validate the entire envelope before publishing any of its records.
        let mut validated = Vec::with_capacity(versions.len());
        for ((name, version), item) in versions.iter().zip(batch.results) {
            if item.module != name.to_string() || item.version != *version {
                bail!("registry batch returned a different coordinate for {name}@{version}");
            }
            match (item.status, item.metadata) {
                (200, Some(metadata)) => {
                    metadata.validate(name, Some(version))?;
                    self.state.check_access(name, &metadata)?;
                    validated.push(Ok(metadata));
                }
                (404, None) => validated.push(Err(anyhow::anyhow!(
                    "Module `{name}` not found or access denied"
                ))),
                _ => bail!("invalid registry batch item for {name}@{version}"),
            }
        }
        Ok(validated)
    }

    async fn split_batch(
        &self,
        versions: &[Coordinate],
    ) -> anyhow::Result<Vec<anyhow::Result<ModuleMetadata>>> {
        if versions.len() == 1 {
            return Ok(self.single_reads(versions).await);
        }
        let (left, right) = versions.split_at(versions.len() / 2);
        // Preserve successful work and per-coordinate diagnostics if one half
        // fails; do not refetch either half through the legacy transport.
        let mut result = Box::pin(self.fetch_batch(left))
            .await
            .unwrap_or_else(|error| repeat_error(error, left.len()));
        result.extend(
            Box::pin(self.fetch_batch(right))
                .await
                .unwrap_or_else(|error| repeat_error(error, right.len())),
        );
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
