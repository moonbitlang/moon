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
