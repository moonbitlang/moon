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

pub fn fuzzy_match<T: AsRef<str>>(
    needle: impl AsRef<str>,
    haystack: impl IntoIterator<Item = T>,
) -> Option<Vec<String>> {
    let mut matcher = nucleo_matcher::Matcher::new(nucleo_matcher::Config::DEFAULT.match_paths());
    let matches = nucleo_matcher::pattern::Pattern::parse(
        needle.as_ref(),
        nucleo_matcher::pattern::CaseMatching::Ignore,
        nucleo_matcher::pattern::Normalization::Smart,
    )
    .match_list(haystack, &mut matcher);
    if matches.is_empty() {
        None
    } else {
        Some(
            matches
                .into_iter()
                .map(|m| m.0.as_ref().to_string())
                .collect(),
        )
    }
}

#[test]
fn test_fuzzy() {
    let haystack = [
        "moonbitlang/core/builtin",
        "moonbitlang/core/int",
        "moonbitlang/core/list",
        "moonbitlang/core/list/internal",
        "moonbitlang/core/hashmap",
    ];
    let result = fuzzy_match("mci", haystack);
    expect_test::expect![[r#"
        Some(
            [
                "moonbitlang/core/int",
                "moonbitlang/core/list/internal",
                "moonbitlang/core/list",
                "moonbitlang/core/builtin",
            ],
        )
    "#]]
    .assert_debug_eq(&result);

    let result = fuzzy_match("moonbitlang/core/list", haystack);
    expect_test::expect![[r#"
        Some(
            [
                "moonbitlang/core/list",
                "moonbitlang/core/list/internal",
            ],
        )
    "#]]
    .assert_debug_eq(&result);
}
