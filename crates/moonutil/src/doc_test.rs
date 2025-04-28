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
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

use crate::common::{backend_filter, MooncOpt, PatchItem, PatchJSON};
use crate::package::Package;

#[derive(Debug)]
pub struct DocTest {
    pub content: String,
    pub file_name: String,
    pub line_number: usize,
    pub line_count: usize,
}

pub struct DocTestExtractor {
    test_pattern: Regex,
}

impl DocTestExtractor {
    pub fn new(is_md_test: bool) -> Self {
        // \r\n for windows, \n for unix
        let pattern = if is_md_test {
            r#"[ \t]*```([^\r\n]*)\s*(?:\r?\n)([\s\S]*?)[ \t]*```"#
        } else {
            r#"///\s*```([^\r\n]*)\s*(?:\r?\n)((?:///.*(?:\r?\n))*?)///\s*```"#
        };

        Self {
            test_pattern: Regex::new(pattern).expect("Invalid regex pattern"),
        }
    }

    pub fn extract_from_file(&self, file_path: &Path) -> anyhow::Result<Vec<DocTest>> {
        let content = fs::read_to_string(file_path)?;

        let mut tests = Vec::new();

        for cap in self.test_pattern.captures_iter(&content) {
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
}

impl PatchJSON {
    pub fn from_doc_tests(doc_tests: Vec<Vec<DocTest>>) -> Self {
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
}

pub fn gen_doc_test_patch(pkg: &Package, moonc_opt: &MooncOpt) -> anyhow::Result<Option<PathBuf>> {
    let mbt_files = backend_filter(
        &pkg.files,
        moonc_opt.build_opt.debug_flag,
        moonc_opt.build_opt.target_backend,
    );

    let mut doc_tests = vec![];
    let doc_test_extractor = DocTestExtractor::new(false);
    for file in mbt_files {
        let doc_test_in_mbt_file = doc_test_extractor.extract_from_file(&file)?;
        if !doc_test_in_mbt_file.is_empty() {
            doc_tests.push(doc_test_in_mbt_file);
        }
    }

    if doc_tests.is_empty() {
        return Ok(None);
    }

    let pj = PatchJSON::from_doc_tests(doc_tests);
    let pj_path = pkg
        .artifact
        .with_file_name(format!("{}.json", crate::common::MOON_DOC_TEST_POSTFIX));
    if !pj_path.parent().unwrap().exists() {
        std::fs::create_dir_all(pj_path.parent().unwrap())?;
    }
    std::fs::write(&pj_path, serde_json_lenient::to_string_pretty(&pj)?)
        .context(format!("failed to write {}", &pj_path.display()))?;

    Ok(Some(pj_path))
}

pub fn gen_md_test_patch(pkg: &Package, moonc_opt: &MooncOpt) -> anyhow::Result<Option<PathBuf>> {
    let md_files = backend_filter(
        &pkg.mbt_md_files,
        moonc_opt.build_opt.debug_flag,
        moonc_opt.build_opt.target_backend,
    );

    let mut md_tests = vec![];
    let md_test_extractor = DocTestExtractor::new(true);
    for file in md_files {
        let doc_test_in_md_file = md_test_extractor.extract_from_file(&file)?;
        if !doc_test_in_md_file.is_empty() {
            md_tests.push(doc_test_in_md_file);
        }
    }

    if md_tests.is_empty() {
        return Ok(None);
    }

    let pj = PatchJSON::from_md_tests(md_tests);
    let pj_path = pkg
        .artifact
        .with_file_name(format!("{}.json", crate::common::MOON_MD_TEST_POSTFIX));
    if !pj_path.parent().unwrap().exists() {
        std::fs::create_dir_all(pj_path.parent().unwrap())?;
    }
    std::fs::write(&pj_path, serde_json_lenient::to_string_pretty(&pj)?)
        .context(format!("failed to write {}", &pj_path.display()))?;

    Ok(Some(pj_path))
}
