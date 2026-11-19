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

//! Interpret captured compiler diagnostics without exposing their wire model.
//!
//! Action output kinds select compiler parsing; other tool output stays opaque.
//! JSON collection retains the original payload, including unknown fields.

mod render;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use ariadne::ReportKind;
use clap::ValueEnum;
use moonbuild_rupes_recta::{
    execution_plan::ActionOutputKind, target_layout::GENERATED_TEST_DRIVER_PREFIX,
};
use moonutil::user_log::UserLog;
use serde::{Deserialize, Serialize};

use super::{BuildConfig, OutputStyle, n2};
use crate::BuildMeta;
use render::DiagnosticSources;

#[derive(Debug, Default, ValueEnum, Clone, PartialEq, Copy, PartialOrd)]
pub enum DiagnosticLevel {
    Info,
    #[value(alias = "warning")]
    Warn,
    #[default]
    Error,
}

impl std::fmt::Display for DiagnosticLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct MooncDiagnostic {
    path: String,
    loc: Loc,
    pub(super) level: String,
    message: String,
    error_code: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    children: Vec<MooncDiagnostic>,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Loc {
    start: Position,
    end: Position,
}

impl Loc {
    fn zero() -> Self {
        Self {
            start: Position { line: 0, col: 0 },
            end: Position { line: 0, col: 0 },
        }
    }

    fn parse_span(text: &str) -> Option<Self> {
        let (start, end) = text.split_once('-')?;

        Some(Self {
            start: Position::parse(start)?,
            end: Position::parse(end)?,
        })
    }

    fn as_range(&self) -> (&Position, &Position) {
        (&self.start, &self.end)
    }
}

impl Serialize for Loc {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&format!(
            "{}:{}-{}:{}",
            self.start.line, self.start.col, self.end.line, self.end.col
        ))
    }
}

impl<'de> Deserialize<'de> for Loc {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        if text.is_empty() {
            return Ok(Self::zero());
        }
        let loc = Self::parse_span(&text)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid location: {text}")))?;
        Ok(loc)
    }
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
struct Position {
    line: usize,
    col: usize,
}

impl Position {
    fn parse(text: &str) -> Option<Self> {
        let (line, col) = text.split_once(':')?;
        Some(Self {
            line: line.parse::<usize>().ok()?,
            col: col.parse::<usize>().ok()?,
        })
    }
}

#[derive(Default)]
pub(super) struct ResultCatcher {
    pub(super) content_writer: Vec<String>,
    n_warnings: usize,
    n_errors: usize,
}

impl ResultCatcher {
    pub(super) fn append_content(&mut self, s: impl Into<String>) {
        self.content_writer.push(s.into());
    }

    fn append_kind(&mut self, kind: Option<ReportKind>) {
        match kind {
            Some(ReportKind::Error) => self.n_errors += 1,
            Some(ReportKind::Warning) => self.n_warnings += 1,
            _ => {}
        }
    }

    fn append_diag(&mut self, diag: &MooncDiagnostic) {
        if diag.level == "error" {
            self.n_errors += 1;
        } else if diag.level == "warning" {
            self.n_warnings += 1;
        }
    }
}

fn should_render_non_diagnostic_build_output(cfg: &BuildConfig, build_succeeded: bool) -> bool {
    !(cfg.suppress_progress && build_succeeded)
}

pub(super) struct CapturedDiagnosticSource<'a> {
    pub(super) diagnostics: &'a ResultCatcher,
    pub(super) output_kind: ActionOutputKind,
    pub(super) build_succeeded: bool,
    pub(super) build_meta: Option<&'a BuildMeta>,
}

pub(super) struct ProcessedDiagnostics {
    pub(super) n_errors: usize,
    pub(super) n_warnings: usize,
    pub(super) hidden_errors: usize,
    pub(super) hidden_warnings: usize,
}

pub(super) struct CollectedJsonDiagnostics {
    pub(super) processed: ProcessedDiagnostics,
    pub(super) diagnostics: Vec<String>,
    pub(super) non_diagnostic_output: Vec<String>,
}

impl ProcessedDiagnostics {
    pub(super) fn warn_if_limited(&self, user_log: &UserLog) {
        if self.hidden_errors != 0 || self.hidden_warnings != 0 {
            user_log.warn(format!(
                "diagnostic output limited by --diagnostic-limit: {} errors and {} warnings were not displayed.",
                self.hidden_errors, self.hidden_warnings
            ));
        }
    }
}

fn rewrite_captured_diagnostic(
    content: &str,
    cfg: &BuildConfig,
    build_meta: Option<&BuildMeta>,
) -> String {
    let Some(meta) = build_meta else {
        return content.to_owned();
    };
    let layout = meta.artifact_paths.target_layout();
    let packages = &meta.resolved_project.pkg_dirs;
    let backend = meta.target_backend();

    if cfg.output_style.needs_moonc_json() {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(content) else {
            return content.to_owned();
        };
        let mut changed = false;
        let mut diagnostics = vec![&mut value];
        while let Some(diagnostic) = diagnostics.pop() {
            let Some(object) = diagnostic.as_object_mut() else {
                continue;
            };
            if let Some(serde_json::Value::String(path)) = object.get_mut("path")
                && let Some(physical) =
                    layout.generated_test_driver_diagnostic_path(packages, Path::new(path), backend)
            {
                *path = physical.to_string_lossy().into_owned();
                changed = true;
            }
            if let Some(serde_json::Value::Array(children)) = object.get_mut("children") {
                diagnostics.extend(children.iter_mut());
            }
        }
        return if changed {
            serde_json::to_string(&value).expect("diagnostic JSON should serialize")
        } else {
            content.to_owned()
        };
    }

    let Some(prefix_start) = content.find(GENERATED_TEST_DRIVER_PREFIX) else {
        return content.to_owned();
    };
    let Some(extension_end) = content[prefix_start..].find(".mbt") else {
        return content.to_owned();
    };
    let path_end = prefix_start + extension_end + ".mbt".len();
    let Some(physical) = layout.generated_test_driver_diagnostic_path(
        packages,
        Path::new(&content[..path_end]),
        backend,
    ) else {
        return content.to_owned();
    };
    format!("{}{}", physical.display(), &content[path_end..])
}

pub(super) fn collect_json_diagnostics(
    sources: &[CapturedDiagnosticSource<'_>],
    cfg: &BuildConfig,
    retain_suppressed_output: bool,
) -> CollectedJsonDiagnostics {
    let mut catcher = ResultCatcher::default();
    for source in sources {
        catcher.n_errors += source.diagnostics.n_errors;
        catcher.n_warnings += source.diagnostics.n_warnings;
    }

    let mut by_file = BTreeMap::<String, BTreeSet<(MooncDiagnostic, String)>>::new();
    let mut non_diagnostic_output = Vec::new();
    for source in sources {
        for content in &source.diagnostics.content_writer {
            let content = if source.output_kind == ActionOutputKind::Compiler {
                rewrite_captured_diagnostic(content, cfg, source.build_meta)
            } else {
                content.clone()
            };
            let diagnostic = match source.output_kind {
                ActionOutputKind::Compiler => {
                    serde_json::from_str::<MooncDiagnostic>(&content).ok()
                }
                ActionOutputKind::Tool => None,
            };
            match diagnostic {
                Some(diagnostic) => {
                    if diagnostic_is_generated_test_driver_warning(&diagnostic) {
                        continue;
                    }
                    by_file
                        .entry(diagnostic.path.clone())
                        .or_default()
                        .insert((diagnostic, content));
                }
                None => {
                    if retain_suppressed_output
                        || should_render_non_diagnostic_build_output(cfg, source.build_succeeded)
                    {
                        non_diagnostic_output.push(content);
                    }
                }
            }
        }
    }

    let mut diagnostics = Vec::new();
    let (hidden_errors, hidden_warnings) = match cfg.diagnostic_limit {
        None => {
            for file_diagnostics in by_file.values() {
                for (diagnostic, content) in file_diagnostics {
                    diagnostics.push(content.clone());
                    catcher.append_diag(diagnostic);
                }
            }
            (0, 0)
        }
        Some(limit) => {
            let mut displayed = 0;
            let mut hidden_errors = 0;
            let mut total_warnings = 0;
            let mut displayed_warnings = 0;
            let mut non_errors = Vec::new();

            for file_diagnostics in by_file.values() {
                for (diagnostic, content) in file_diagnostics {
                    if diagnostic_is_error(diagnostic) {
                        if displayed < limit {
                            diagnostics.push(content.clone());
                            catcher.append_diag(diagnostic);
                            displayed += 1;
                        } else {
                            hidden_errors += 1;
                        }
                        continue;
                    }

                    if diagnostic_is_warning(diagnostic) {
                        total_warnings += 1;
                    }
                    if displayed < limit {
                        non_errors.push((diagnostic, content));
                    }
                }
            }

            if displayed < limit {
                for (diagnostic, content) in non_errors {
                    diagnostics.push(content.clone());
                    catcher.append_diag(diagnostic);
                    displayed += 1;
                    if diagnostic_is_warning(diagnostic) {
                        displayed_warnings += 1;
                    }
                    if displayed == limit {
                        break;
                    }
                }
            }

            let hidden_warnings = total_warnings - displayed_warnings;
            catcher.n_errors += hidden_errors;
            catcher.n_warnings += hidden_warnings;
            (hidden_errors, hidden_warnings)
        }
    };

    CollectedJsonDiagnostics {
        processed: ProcessedDiagnostics {
            n_errors: catcher.n_errors,
            n_warnings: catcher.n_warnings,
            hidden_errors,
            hidden_warnings,
        },
        diagnostics,
        non_diagnostic_output,
    }
}

// TODO: Return output or accept writers when the remaining build commands
// migrate to ADR 0004; preserve the current stdout/stderr routing until then.
pub(super) fn process_captured_diagnostics(
    sources: &[CapturedDiagnosticSource<'_>],
    cfg: &BuildConfig,
) -> ProcessedDiagnostics {
    if cfg.output_style == OutputStyle::Json {
        let collected = collect_json_diagnostics(sources, cfg, false);
        for content in &collected.non_diagnostic_output {
            eprintln!("{content}");
        }
        for content in &collected.diagnostics {
            println!("{content}");
        }
        return collected.processed;
    }

    let mut catcher = ResultCatcher::default();
    for source in sources {
        catcher.n_errors += source.diagnostics.n_errors;
        catcher.n_warnings += source.diagnostics.n_warnings;
    }
    let mut hidden_errors_total = 0;
    let mut hidden_warnings_total = 0;
    let captured = sources.iter().flat_map(|source| {
        source
            .diagnostics
            .content_writer
            .iter()
            .map(move |content| {
                (
                    if source.output_kind == ActionOutputKind::Compiler {
                        rewrite_captured_diagnostic(content, cfg, source.build_meta)
                    } else {
                        content.clone()
                    },
                    source.build_succeeded,
                    source.output_kind,
                )
            })
    });

    match cfg.output_style {
        OutputStyle::Json => unreachable!(),
        OutputStyle::Fancy => {
            let mut by_file = BTreeMap::<String, BTreeSet<MooncDiagnostic>>::new();
            for (content, build_succeeded, output_kind) in captured {
                let diagnostic = match output_kind {
                    ActionOutputKind::Compiler => {
                        serde_json::from_str::<MooncDiagnostic>(&content).ok()
                    }
                    ActionOutputKind::Tool => None,
                };
                match diagnostic {
                    Some(d) => {
                        if diagnostic_is_generated_test_driver_warning(&d) {
                            continue;
                        }
                        by_file.entry(d.path.clone()).or_default().insert(d);
                    }
                    None => {
                        // Non-diagnostics output, just print as-is
                        // This could happen for installing binaries dependencies etc.
                        if should_render_non_diagnostic_build_output(cfg, build_succeeded) {
                            eprintln!("{content}");
                        }
                    }
                };
            }

            let patch_file = cfg.patch_file.as_ref();
            let mut sources = DiagnosticSources::default();
            match cfg.diagnostic_limit {
                None => {
                    for file_diagnostics in by_file.values() {
                        for diag in file_diagnostics {
                            let kind = diag.render_diagnostics(
                                n2::use_fancy(),
                                patch_file,
                                &mut sources,
                                cfg.explain_errors,
                                cfg.render_no_loc,
                            );
                            catcher.append_kind(kind);
                        }
                    }
                }
                Some(limit) => {
                    let build_config = cfg;
                    let mut displayed = 0;
                    let mut hidden_errors = 0;
                    let mut total_warnings = 0;
                    let mut displayed_warnings = 0;
                    let mut non_errors = Vec::new();

                    for file_diagnostics in by_file.values() {
                        for diag in file_diagnostics {
                            if !diagnostic_is_renderable(diag, build_config) {
                                continue;
                            }

                            if diagnostic_is_error(diag) {
                                if displayed < limit {
                                    let kind = diag.render_diagnostics(
                                        n2::use_fancy(),
                                        patch_file,
                                        &mut sources,
                                        build_config.explain_errors,
                                        build_config.render_no_loc,
                                    );
                                    catcher.append_kind(kind);
                                    displayed += 1;
                                } else {
                                    hidden_errors += 1;
                                }
                                continue;
                            }

                            if diagnostic_is_warning(diag) {
                                total_warnings += 1;
                            }
                            if displayed < limit {
                                non_errors.push(diag);
                            }
                        }
                    }

                    if displayed < limit {
                        for diag in non_errors {
                            let kind = diag.render_diagnostics(
                                n2::use_fancy(),
                                patch_file,
                                &mut sources,
                                build_config.explain_errors,
                                build_config.render_no_loc,
                            );
                            catcher.append_kind(kind);
                            displayed += 1;
                            if diagnostic_is_warning(diag) {
                                displayed_warnings += 1;
                            }
                            if displayed == limit {
                                break;
                            }
                        }
                    }

                    let hidden_warnings = total_warnings - displayed_warnings;
                    hidden_errors_total += hidden_errors;
                    hidden_warnings_total += hidden_warnings;
                    catcher.n_errors += hidden_errors;
                    catcher.n_warnings += hidden_warnings;
                }
            }
        }
        OutputStyle::Raw => {
            for (content, _, _) in captured {
                println!("{content}");
            }
        }
    }
    ProcessedDiagnostics {
        n_errors: catcher.n_errors,
        n_warnings: catcher.n_warnings,
        hidden_errors: hidden_errors_total,
        hidden_warnings: hidden_warnings_total,
    }
}

fn diagnostic_is_error(diag: &MooncDiagnostic) -> bool {
    diag.level == "error"
}

fn diagnostic_is_warning(diag: &MooncDiagnostic) -> bool {
    matches!(diag.level.as_str(), "warn" | "warning")
}

fn diagnostic_is_generated_test_driver_warning(diag: &MooncDiagnostic) -> bool {
    diagnostic_is_warning(diag) && diag.path.contains("__generated_driver_for_")
}

fn diagnostic_is_renderable(diag: &MooncDiagnostic, cfg: &BuildConfig) -> bool {
    if !diag.path.is_empty() {
        return true;
    }

    DiagnosticLevel::from_str(&diag.level, true).is_ok_and(|level| level >= cfg.render_no_loc)
}

#[cfg(test)]
mod tests;
