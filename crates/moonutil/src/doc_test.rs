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

use anyhow::Context;
use colored::Colorize;
use regex::Regex;
use std::fs;
use std::path::Path;

use crate::common::{backend_filter, MooncOpt, PatchItem, PatchJSON};
use crate::package::Package;

use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};

#[derive(Debug)]
pub struct DocTest {
    pub content: String,
    pub file_name: String,
    pub line_number: usize,
    pub line_count: usize,
}

#[derive(Default)]
pub struct DocTestExtractor {}

impl DocTestExtractor {
    pub fn extract_doc_test_from_file(&self, file_path: &Path) -> anyhow::Result<Vec<DocTest>> {
        let content = fs::read_to_string(file_path)?;

        let mut tests = Vec::new();

        // \r\n for windows, \n for unix
        let pattern =
            Regex::new(r#"///\s*```([^\r\n]*)\s*(?:\r?\n)((?:///.*(?:\r?\n))*?)///\s*```"#)
                .expect("Invalid regex pattern");
        for cap in pattern.captures_iter(&content) {
            if let Some(test_match) = cap.get(0) {
                let lang = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                if lang.is_empty() || lang == "mbt" || lang == "moonbit" {
                    let line_number = content[..test_match.start()]
                        .chars()
                        .filter(|&c| c == '\n')
                        .count()
                        + 1;

                    if let Some(test_content) = cap.get(2) {
                        let line_count = test_content.as_str().lines().count();

                        tests.push(DocTest {
                            content: test_content.as_str().to_string(),
                            file_name: file_path.file_name().unwrap().to_str().unwrap().to_string(),
                            line_number,
                            line_count,
                        });
                    }
                }
            }
        }

        Ok(tests)
    }

    pub fn extract_md_test_from_file(&self, file_path: &Path) -> anyhow::Result<Vec<DocTest>> {
        let content = fs::read_to_string(file_path)?;

        let mut tests = Vec::new();

        let parser = Parser::new(&content);

        let mut current_code = String::new();
        let mut in_moonbit_block = false;
        let mut block_start_line = 0;
        let mut current_indent = String::new();

        for (event, range) in parser.into_offset_iter() {
            let current_line = content[..range.start]
                .chars()
                .filter(|c| *c == '\n')
                .count()
                + 1;

            match event {
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang))) => {
                    if lang.as_ref() == "moonbit" || lang.as_ref() == "mbt" {
                        in_moonbit_block = true;
                        block_start_line = current_line;
                        current_code.clear();
                        current_indent = content
                            .lines()
                            .nth(current_line - 1)
                            .unwrap_or("")
                            .split("`")
                            .next()
                            .unwrap_or("")
                            .to_string();
                    }
                }
                Event::Text(text) if in_moonbit_block => {
                    current_code.push_str(&current_indent);
                    current_code.push_str(&text);
                }
                Event::End(_) if in_moonbit_block => {
                    tests.push(DocTest {
                        content: current_code.clone(),
                        file_name: file_path.file_name().unwrap().to_str().unwrap().to_string(),
                        line_number: block_start_line,
                        line_count: current_code.lines().count(),
                    });
                    in_moonbit_block = false;
                }
                _ => {}
            }
        }

        Ok(tests)
    }
}

impl PatchJSON {
    pub fn from_doc_tests(doc_tests: Vec<Vec<DocTest>>, pkg_dir: &Path) -> Self {
        let mut patches = vec![];
        for doc_tests_in_mbt_file in doc_tests.iter() {
            let mut current_line = 1;
            let mut content = String::new();
            for doc_test in doc_tests_in_mbt_file {
                let test_name = format!(
                    "{} {} {} {}",
                    "doc_test", doc_test.file_name, doc_test.line_number, doc_test.line_count
                );

                let already_wrapped = doc_test
                    .content
                    .lines()
                    .any(|line| line.replacen("///", "", 1).trim_start().starts_with("test"));

                if already_wrapped {
                    eprintln!(
                        "{}: don't need to wrap code in test block at {}:{}",
                        "Warning".yellow(),
                        pkg_dir.join(&doc_test.file_name).display(),
                        doc_test.line_number
                    );
                }

                let processed_content = doc_test
                    .content
                    .as_str()
                    .lines()
                    .map(|line| {
                        if already_wrapped {
                            let remove_slash = line.replacen("///", "", 1).trim_start().to_string();
                            if remove_slash.starts_with("test") || remove_slash.starts_with("}") {
                                remove_slash
                            } else {
                                line.to_string().replacen("///", "   ", 1)
                            }
                        } else {
                            format!("   {}", line.trim_start_matches("///")).to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                let start_line_number = doc_test.line_number;
                let empty_lines = "\n".repeat(start_line_number - current_line);

                if already_wrapped {
                    content.push_str(&format!("\n{}{}\n", empty_lines, processed_content));
                } else {
                    content.push_str(&format!(
                        "{}test \"{}\" {{\n{}\n}}",
                        empty_lines, test_name, processed_content
                    ));
                }

                // +1 for the }
                current_line = start_line_number + doc_test.line_count + 1;

                // this is for debug
                // std::fs::write(format!("__doc_test_{}.mbt", doc_test.file_name), &content).unwrap();
            }

            patches.push(PatchItem {
                // xxx.mbt -> xxx_doc_test.mbt
                name: format!(
                    "{}{}.mbt",
                    doc_tests_in_mbt_file[0].file_name.trim_end_matches(".mbt"),
                    crate::common::MOON_DOC_TEST_POSTFIX,
                ),
                content,
            });
        }

        PatchJSON {
            drops: vec![],
            patches,
        }
    }

    pub fn from_md_tests(md_tests: Vec<Vec<DocTest>>) -> Self {
        let mut patches = vec![];
        for doc_test_in_md_file in md_tests.iter() {
            let mut current_line = 1;
            let mut content = String::new();
            for md_test in doc_test_in_md_file {
                let processed_content = md_test
                    .content
                    .as_str()
                    .lines()
                    .collect::<Vec<_>>()
                    .join("\n");

                let start_line_number = md_test.line_number;
                let empty_lines = "\n".repeat(start_line_number - current_line);

                content.push_str(&format!("\n{}{}\n", empty_lines, processed_content));

                // +1 for the }
                current_line = start_line_number + md_test.line_count + 1;

                // this is for debug
                // std::fs::write(format!("__md_test_{}.mbt", md_test.file_name), &content).unwrap();
            }
            patches.push(PatchItem {
                // xxx.md -> xxx_md_test.mbt
                name: doc_test_in_md_file[0].file_name.clone(),
                content,
            });
        }

        PatchJSON {
            drops: vec![],
            patches,
        }
    }

    pub fn write_to_path(&self, path: &Path) -> anyhow::Result<()> {
        if !path.parent().unwrap().exists() {
            std::fs::create_dir_all(path.parent().unwrap()).with_context(|| {
                format!(
                    "failed to create directory {}",
                    path.parent().unwrap().display()
                )
            })?;
        }
        let content = serde_json_lenient::to_string_pretty(self)?;
        std::fs::write(path, content)
            .with_context(|| format!("failed to write to {}", path.display()))?;
        Ok(())
    }

    pub fn merge_patches(
        md_test_patch: Option<PatchJSON>,
        doc_test_patch: Option<PatchJSON>,
    ) -> Option<Self> {
        match (md_test_patch, doc_test_patch) {
            (Some(md), Some(doc)) => {
                let mut patches = md.patches;
                patches.extend(doc.patches);
                Some(PatchJSON {
                    drops: vec![],
                    patches,
                })
            }
            (None, Some(doc)) => Some(doc),
            (Some(md), None) => Some(md),
            (None, None) => None,
        }
    }
}

pub fn gen_doc_test_patch(
    pkg: &Package,
    moonc_opt: &MooncOpt,
) -> anyhow::Result<Option<PatchJSON>> {
    let mbt_files = backend_filter(
        &pkg.files,
        moonc_opt.build_opt.debug_flag,
        moonc_opt.build_opt.target_backend,
    );

    let mut doc_tests = vec![];
    let doc_test_extractor = DocTestExtractor::default();
    for file in mbt_files {
        let doc_test_in_mbt_file = doc_test_extractor.extract_doc_test_from_file(&file)?;
        if !doc_test_in_mbt_file.is_empty() {
            doc_tests.push(doc_test_in_mbt_file);
        }
    }

    if doc_tests.is_empty() {
        return Ok(None);
    }

    let pj = PatchJSON::from_doc_tests(doc_tests, &pkg.root_path);
    Ok(Some(pj))
}

pub fn gen_md_test_patch(pkg: &Package, moonc_opt: &MooncOpt) -> anyhow::Result<Option<PatchJSON>> {
    let md_files = backend_filter(
        &pkg.mbt_md_files,
        moonc_opt.build_opt.debug_flag,
        moonc_opt.build_opt.target_backend,
    );

    let mut md_tests = vec![];
    let md_test_extractor = DocTestExtractor::default();
    for file in md_files {
        let doc_test_in_md_file = md_test_extractor.extract_md_test_from_file(&file)?;
        if !doc_test_in_md_file.is_empty() {
            md_tests.push(doc_test_in_md_file);
        }
    }

    if md_tests.is_empty() {
        return Ok(None);
    }

    let pj = PatchJSON::from_md_tests(md_tests);
    Ok(Some(pj))
}
