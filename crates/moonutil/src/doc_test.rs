use anyhow::Context;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

use crate::common::{backend_filter, MooncOpt};
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
    pub fn new() -> Self {
        // \r\n for windows, \n for unix
        let pattern = r#"///\s*```(?:\r?\n)((?:///.*(?:\r?\n))*?)///\s*```"#;
        Self {
            test_pattern: Regex::new(pattern).expect("Invalid regex pattern"),
        }
    }

    pub fn extract_from_file(&self, file_path: &Path) -> anyhow::Result<Vec<DocTest>> {
        let content = fs::read_to_string(file_path)?;

        let mut tests = Vec::new();

        for cap in self.test_pattern.captures_iter(&content) {
            if let Some(test_match) = cap.get(0) {
                let line_number = content[..test_match.start()]
                    .chars()
                    .filter(|&c| c == '\n')
                    .count()
                    + 1;

                if let Some(test_content) = cap.get(1) {
                    let processed_content = test_content
                        .as_str()
                        .lines()
                        .map(|line| format!("    {}", line.trim_start_matches("/// ")).to_string())
                        .collect::<Vec<_>>()
                        .join("\n");

                    let line_count = processed_content.split('\n').count();

                    tests.push(DocTest {
                        content: processed_content,
                        file_name: file_path.file_name().unwrap().to_str().unwrap().to_string(),
                        line_number,
                        line_count,
                    });
                }
            }
        }

        Ok(tests)
    }
}

#[derive(Debug, serde::Serialize)]
pub struct PatchJSON {
    pub drops: Vec<String>,
    pub patches: Vec<PatchItem>,
}

#[derive(Debug, serde::Serialize)]
pub struct PatchItem {
    pub name: String,
    pub content: String,
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

                let start_line_number = doc_test.line_number;
                let empty_lines = "\n".repeat(start_line_number - current_line);

                content.push_str(&format!(
                    "{}test \"{}\" {{\n{}\n}}",
                    empty_lines, test_name, doc_test.content
                ));

                std::fs::write(format!("__doc_test_{}.mbt", doc_test.file_name), &content).unwrap();

                // +1 for the }
                current_line = start_line_number + doc_test.line_count + 1;
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
}

pub fn gen_doc_test_patch(pkg: &Package, moonc_opt: &MooncOpt) -> anyhow::Result<PathBuf> {
    let mbt_files = backend_filter(
        &pkg.files,
        moonc_opt.build_opt.debug_flag,
        moonc_opt.build_opt.target_backend,
    );

    let mut doc_tests = vec![];
    let doc_test_extractor = DocTestExtractor::new();
    for file in mbt_files {
        let doc_test_in_mbt_file = doc_test_extractor.extract_from_file(&file)?;
        if !doc_test_in_mbt_file.is_empty() {
            doc_tests.push(doc_test_in_mbt_file);
        }
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

    Ok(pj_path)
}
