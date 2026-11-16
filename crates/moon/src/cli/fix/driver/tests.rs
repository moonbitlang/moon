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

use super::*;

const FILE: &str = "/p/a.mbt";

fn edit(loc: &str, expect: &str, new_text: &str) -> Edit {
    Edit {
        path: PathBuf::from(FILE),
        loc: loc.to_owned(),
        expect: expect.to_owned(),
        new_text: new_text.to_owned(),
        fresh: Vec::new(),
        inside_literal: false,
        followed_by: None,
    }
}

fn fix(title: &str, edits: Vec<Edit>) -> Fix {
    Fix {
        title: title.to_owned(),
        edits,
    }
}

/// What a plan does, in a form that is easy to compare.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    /// The new content of each changed file.
    fixed: Vec<(String, String)>,
    /// The title of each skipped fix and why it was skipped.
    skipped: Vec<(String, SkipReason)>,
}

fn outcome(plan: FixPlan) -> Outcome {
    for change in plan.changes.values() {
        assert_ne!(change.original, change.fixed);
    }
    Outcome {
        fixed: plan
            .changes
            .into_iter()
            .map(|(path, change)| (path.display().to_string(), change.fixed))
            .collect(),
        skipped: plan
            .skipped
            .into_iter()
            .map(|skipped| (skipped.fix.title, skipped.reason))
            .collect(),
    }
}

/// Plans `fixes` proposed by the single backend that compiled `FILE`, which
/// reads `text`.
fn plan_one(text: &str, fixes: Vec<Fix>) -> Outcome {
    plan_with(
        &[(FILE, text)],
        &[("wasm-gc", fixes)],
        &[(FILE, &["wasm-gc"])],
    )
}

fn plan_with(
    files: &[(&str, &str)],
    proposals: &[(&'static str, Vec<Fix>)],
    compiled: &[(&str, &[&'static str])],
) -> Outcome {
    let proposals = proposals
        .iter()
        .map(|(backend, fixes)| (*backend, fixes.iter().cloned().collect()))
        .collect();
    let compiled = compiled
        .iter()
        .map(|(path, backends)| (PathBuf::from(path), backends.iter().copied().collect()))
        .collect();
    let files: BTreeMap<PathBuf, String> = files
        .iter()
        .map(|(path, text)| (PathBuf::from(path), text.to_string()))
        .collect();
    outcome(plan_fixes(&proposals, &compiled, |path| {
        files
            .get(path)
            .cloned()
            .ok_or_else(|| std::io::ErrorKind::NotFound.into())
    }))
}

fn applied(text: &str) -> Outcome {
    Outcome {
        fixed: vec![(FILE.to_owned(), text.to_owned())],
        skipped: vec![],
    }
}

fn skipped(title: &str, reason: SkipReason) -> Outcome {
    Outcome {
        fixed: vec![],
        skipped: vec![(title.to_owned(), reason)],
    }
}

#[test]
fn applies_a_fix_whose_range_reads_its_expect_text() {
    let remove_mut = fix("Remove `mut`", vec![edit("2:3-2:11", "let mut ", "let ")]);
    assert_eq!(
        plan_one(
            "fn f() -> Int {\n  let mut x = 1\n  x\n}\n",
            vec![remove_mut]
        ),
        applied("fn f() -> Int {\n  let x = 1\n  x\n}\n")
    );
}

#[test]
fn reads_the_first_fix_of_a_diagnostic() {
    let diagnostic = serde_json::json!({
        "path": FILE,
        "fixes": [
            {"title": "first", "edits": [{"path": FILE, "loc": "1:1-1:2", "expect": "a",
              "new_text": "b", "fresh": ["x"], "inside_literal": true, "followed_by": ""}]},
            {"title": "second", "edits": []}
        ]
    });
    let mut expected = fix("first", vec![edit("1:1-1:2", "a", "b")]);
    expected.edits[0].fresh = vec!["x".to_owned()];
    expected.edits[0].inside_literal = true;
    expected.edits[0].followed_by = Some(String::new());
    assert_eq!(Fix::preferred_of_diagnostic(&diagnostic), Some(expected));
    assert_eq!(
        Fix::preferred_of_diagnostic(&serde_json::json!({"path": FILE})),
        None
    );
    assert_eq!(
        Fix::preferred_of_diagnostic(&serde_json::json!({"fixes": []})),
        None
    );
}

#[cfg(windows)]
#[test]
fn diagnostic_paths_use_the_same_identity_with_or_without_a_verbatim_prefix() {
    let diagnostic = |path: &str| {
        serde_json::json!({
            "fixes": [{"title": "rename", "edits": [{
                "path": path, "loc": "1:1-1:2", "expect": "x", "new_text": "y"
            }]}]
        })
    };
    let plain = Fix::preferred_of_diagnostic(&diagnostic(r"C:\p\a.mbt")).unwrap();
    let verbatim = Fix::preferred_of_diagnostic(&diagnostic(r"\\?\C:\p\a.mbt")).unwrap();
    assert_eq!(plain.edits[0].path, PathBuf::from(r"C:\p\a.mbt"));
    assert_eq!(plain, verbatim);
}

#[test]
fn counts_columns_in_code_points_and_applies_edits_right_to_left() {
    // `é` and `😀` take several bytes but one column each.
    let text = "let é = 😀 + aa + bb\n";
    let fixes = vec![
        fix("first", vec![edit("1:13-1:15", "aa", "a")]),
        fix("second", vec![edit("1:18-1:20", "bb", "b")]),
    ];
    assert_eq!(plan_one(text, fixes), applied("let é = 😀 + a + b\n"));
}

#[test]
fn compares_expect_text_with_runs_of_blanks_collapsed() {
    let fixes = vec![fix("tabs", vec![edit("1:1-1:10", "let mut ", "let ")])];
    assert_eq!(plan_one("let\t mut x = 1\n", fixes), applied("let x = 1\n"));
}

// Check 1: one line, inside the line, reading `expect`.

#[test]
fn skips_an_edit_whose_range_does_not_read_expect() {
    // The file changed since the check: the edit would remove the wrong text.
    let fixes = vec![fix("stale", vec![edit("1:5-1:9", "mut ", "")])];
    assert_eq!(
        plan_one("let x = 1\n", fixes),
        skipped("stale", SkipReason::ExpectMismatch)
    );
}

#[test]
fn skips_edits_that_are_not_within_one_line() {
    let text = "let mut x = 1\nx\n";
    for loc in [
        "1:5-2:1",   // spans two lines
        "1:5-1:99",  // ends past the line
        "1:0-1:2",   // column 0
        "1:5-1:4",   // ends before it starts
        "4:1-4:1",   // past the last line (line 3 is the empty one after "\n")
        "0:1-0:1",   // line 0
        "1:5_1:9",   // malformed
        "1:5-1:+9",  // malformed number
        "1:5-1:9:1", // malformed position
    ] {
        let fixes = vec![fix(loc, vec![edit(loc, "mut ", "")])];
        assert_eq!(
            plan_one(text, fixes),
            skipped(loc, SkipReason::BadLocation),
            "{loc}"
        );
    }
}

#[test]
fn accepts_an_edit_just_past_the_last_character_of_a_line() {
    let fixes = vec![fix("append", vec![edit("1:5-1:5", "", ";")])];
    assert_eq!(plan_one("f(x)\n", fixes), applied("f(x);\n"));
}

// Check 2: no two identifier characters become adjacent.

#[test]
fn skips_an_edit_that_joins_two_identifiers() {
    // Deleting `@pkg.` from `return@pkg.C` would produce `returnC`.
    let fixes = vec![fix("join", vec![edit("1:7-1:12", "@pkg.", "")])];
    assert_eq!(
        plan_one("return@pkg.C\n", fixes),
        skipped("join", SkipReason::JoinsTokens)
    );
}

#[test]
fn treats_non_ascii_characters_as_identifier_characters() {
    let fixes = vec![fix("join", vec![edit("1:2-1:3", " ", "")])];
    assert_eq!(
        plan_one("a é\n", fixes),
        skipped("join", SkipReason::JoinsTokens)
    );
}

#[test]
fn accepts_an_edit_that_keeps_identifiers_already_adjacent() {
    // `foo` -> `bar`: the new text touches only what the old text touched.
    let fixes = vec![fix("rename", vec![edit("1:5-1:8", "foo", "bar")])];
    assert_eq!(plan_one("let foo = 1\n", fixes), applied("let bar = 1\n"));
}

#[test]
fn skips_the_adjacency_check_inside_a_literal() {
    let mut escape = edit("1:3-1:4", " ", "");
    escape.inside_literal = true;
    let fixes = vec![fix("literal", vec![escape])];
    assert_eq!(plan_one("\"a b\"\n", fixes), applied("\"ab\"\n"));
}

#[test]
fn checks_touching_edits_of_one_fix_on_their_combined_result() {
    // Each edit of `joins` alone keeps `a` and `b` apart, but applying both
    // turns `a -b` into `ab`.
    let apart = vec![fix(
        "apart",
        vec![edit("1:2-1:3", " ", ""), edit("1:3-1:4", "-", " ")],
    )];
    assert_eq!(plan_one("a -b\n", apart), applied("a b\n"));
    let joins = vec![fix(
        "joins",
        vec![edit("1:2-1:3", " ", ""), edit("1:3-1:4", "-", "")],
    )];
    assert_eq!(
        plan_one("a -b\n", joins),
        skipped("joins", SkipReason::JoinsTokens)
    );
}

#[test]
fn checks_complete_touching_groups_instead_of_intermediate_prefixes() {
    // The first two deletions alone join `a` and `b`; the third edit restores
    // their separator. The whole fix is safe, regardless of proposal order.
    let edits = vec![
        edit("1:2-1:3", " ", ""),
        edit("1:3-1:4", "-", ""),
        edit("1:4-1:5", "b", " b"),
    ];
    for edits in [edits.clone(), edits.into_iter().rev().collect()] {
        assert_eq!(
            plan_one("a -b\n", vec![fix("apart", edits)]),
            applied("a b\n")
        );
    }
}

#[test]
fn combined_touching_edits_skip_the_adjacency_check_only_if_all_are_literal() {
    let literal = |loc, expect, new_text| {
        let mut e = edit(loc, expect, new_text);
        e.inside_literal = true;
        e
    };
    let all_literal = vec![fix(
        "all",
        vec![literal("1:3-1:4", " ", ""), literal("1:4-1:5", "-", "")],
    )];
    assert_eq!(plan_one("\"a -b\"\n", all_literal), applied("\"ab\"\n"));

    let mixed = vec![fix(
        "mixed",
        vec![literal("1:3-1:4", " ", ""), edit("1:4-1:5", "-", "")],
    )];
    assert_eq!(
        plan_one("\"a -b\"\n", mixed),
        skipped("mixed", SkipReason::JoinsTokens)
    );
}

// Check 3: `fresh` names.

#[test]
fn rejects_fresh_names_introduced_by_another_fix_in_either_order() {
    // Even an edit without its own freshness constraint can invalidate the
    // other fix's constraint. Selection must protect both directions.
    for (first_fresh, second_fresh) in [(true, true), (true, false), (false, true)] {
        let mut first = edit("1:5-1:6", "x", "y");
        let mut second = edit("2:5-2:6", "z", "y");
        if first_fresh {
            first.fresh.push("y".to_owned());
        }
        if second_fresh {
            second.fresh.push("y".to_owned());
        }
        assert_eq!(
            plan_one(
                "let x = 1\nlet z = 2\n",
                vec![fix("first", vec![first]), fix("second", vec![second])]
            ),
            Outcome {
                fixed: vec![(FILE.to_owned(), "let y = 1\nlet z = 2\n".to_owned())],
                skipped: vec![("second".to_owned(), SkipReason::ConflictsWithSelectedFix)],
            }
        );
    }
}

#[test]
fn fresh_names_remain_local_to_a_file_and_may_be_reused_within_one_fix() {
    let mut first = edit("1:5-1:6", "x", "y");
    first.fresh.push("y".to_owned());
    let mut second = edit("2:1-2:2", "x", "y");
    second.fresh.push("y".to_owned());
    assert_eq!(
        plan_one(
            "let x = 1\nx\n",
            vec![fix("rename", vec![first.clone(), second])]
        ),
        applied("let y = 1\ny\n")
    );
    let mut other_file = first.clone();
    other_file.path = PathBuf::from("/p/b.mbt");
    assert_eq!(
        plan_with(
            &[(FILE, "let x = 1\n"), ("/p/b.mbt", "let x = 1\n")],
            &[(
                "wasm-gc",
                vec![fix("first", vec![first]), fix("second", vec![other_file])]
            )],
            &[(FILE, &["wasm-gc"]), ("/p/b.mbt", &["wasm-gc"])],
        ),
        Outcome {
            fixed: vec![
                (FILE.to_owned(), "let y = 1\n".to_owned()),
                ("/p/b.mbt".to_owned(), "let y = 1\n".to_owned())
            ],
            skipped: vec![],
        }
    );
}

#[test]
fn skips_a_fix_whose_fresh_name_occurs_as_an_identifier() {
    let mut rename = edit("1:5-1:6", "x", "y");
    rename.fresh = vec!["y".to_owned()];
    let fixes = vec![fix("fresh", vec![rename.clone()])];
    assert_eq!(
        plan_one("let x = 1\nlet z = y\n", fixes),
        skipped("fresh", SkipReason::NameNotFresh)
    );

    // `y` occurs only inside longer identifiers, so it is still fresh.
    let fixes = vec![fix("fresh", vec![rename])];
    assert_eq!(
        plan_one("let x = 1\nlet yy = _y + y2\n", fixes),
        applied("let y = 1\nlet yy = _y + y2\n")
    );
}

// Check 4: `followed_by`.

#[test]
fn checks_the_text_after_an_edit_against_followed_by() {
    let insert = |followed_by: &str| {
        let mut e = edit("1:7-1:7", "", ",");
        e.followed_by = Some(followed_by.to_owned());
        vec![fix("insert", vec![e])]
    };
    assert_eq!(
        plan_one("{ x: 1  }\n", insert("}")),
        applied("{ x: 1,  }\n")
    );
    assert_eq!(
        plan_one("{ x: 1  ]\n", insert("}")),
        skipped("insert", SkipReason::NotFollowedBy)
    );
    // `""`: only blanks (and a CR) may follow on the line.
    assert_eq!(
        plan_one("{ x: 1 \t\r\n", insert("")),
        applied("{ x: 1, \t\r\n")
    );
    assert_eq!(
        plan_one("{ x: 1  }\n", insert("")),
        skipped("insert", SkipReason::NotFollowedBy)
    );
}

// Check 5: overlapping edits and fixes.

#[test]
fn skips_a_fix_whose_edits_overlap_or_start_at_the_same_point() {
    let overlap = vec![fix(
        "overlap",
        vec![edit("1:1-1:3", "ab", "x"), edit("1:2-1:4", "bc", "y")],
    )];
    assert_eq!(
        plan_one("abcd\n", overlap),
        skipped("overlap", SkipReason::OverlappingEdits)
    );
    let same_start = vec![fix(
        "same start",
        vec![edit("1:2-1:2", "", "("), edit("1:2-1:2", "", ")")],
    )];
    assert_eq!(
        plan_one("a b\n", same_start),
        skipped("same start", SkipReason::OverlappingEdits)
    );
}

#[test]
fn skips_a_fix_that_conflicts_with_one_selected_before_it() {
    let text = "let mut x = 1\n";
    let first = fix("first", vec![edit("1:5-1:9", "mut ", "")]);
    for (title, loc, expect) in [
        ("overlaps", "1:7-1:10", "t x"),
        ("same start", "1:5-1:5", ""),
    ] {
        let second = fix(title, vec![edit(loc, expect, "(")]);
        assert_eq!(
            plan_one(text, vec![first.clone(), second]),
            Outcome {
                fixed: vec![(FILE.to_owned(), "let x = 1\n".to_owned())],
                skipped: vec![(title.to_owned(), SkipReason::ConflictsWithSelectedFix)],
            },
            "{title}"
        );
    }
}

// Check 6: files whose positions moonc may count differently.

#[test]
fn applies_no_fix_in_a_file_with_unusual_line_breaks_or_long_lines() {
    let long_line = format!("let mut x = 1\n{}\n", "a".repeat(65535));
    for text in [
        "let mut x = 1\rx\n",
        "let mut x = 1\nx\u{2028}\n",
        "let mut x = 1\nx\u{2029}\n",
        long_line.as_str(),
    ] {
        let fixes = vec![fix("remove", vec![edit("1:5-1:9", "mut ", "")])];
        assert_eq!(
            plan_one(text, fixes),
            skipped("remove", SkipReason::UnreliablePositions)
        );
    }
}

#[test]
fn applies_fixes_in_a_crlf_file_and_below_the_line_length_limit() {
    let fixes = vec![fix("remove", vec![edit("2:5-2:9", "mut ", "")])];
    assert_eq!(
        plan_one("x\r\nlet mut x = 1\r\n", fixes),
        applied("x\r\nlet x = 1\r\n")
    );
    let line = "a".repeat(65534);
    let fixes = vec![fix("remove", vec![edit("1:5-1:9", "mut ", "")])];
    assert_eq!(
        plan_one(&format!("let mut x = 1\n{line}\n"), fixes),
        applied(&format!("let x = 1\n{line}\n"))
    );
}

// Agreement between target backends.

#[test]
fn applies_a_fix_only_if_every_backend_that_compiled_the_file_proposes_it() {
    let text = "let mut x = 1\n";
    let remove = fix("remove", vec![edit("1:5-1:9", "mut ", "")]);
    let both: &[&str] = &["js", "wasm-gc"];
    assert_eq!(
        plan_with(
            &[(FILE, text)],
            &[("js", vec![remove.clone()]), ("wasm-gc", vec![])],
            &[(FILE, both)],
        ),
        skipped("remove", SkipReason::NotProposedByEveryTarget)
    );
    assert_eq!(
        plan_with(
            &[(FILE, text)],
            &[
                ("js", vec![remove.clone()]),
                ("wasm-gc", vec![remove.clone()])
            ],
            &[(FILE, both)],
        ),
        applied("let x = 1\n")
    );
    // A backend that did not compile the file has no say.
    assert_eq!(
        plan_with(
            &[(FILE, text)],
            &[("js", vec![remove.clone()]), ("wasm-gc", vec![])],
            &[(FILE, &["js"]), ("/p/b.mbt", &["wasm-gc"])],
        ),
        applied("let x = 1\n")
    );
}

#[test]
fn never_edits_a_file_that_no_check_compiled() {
    let text = "let mut x = 1\n";
    let remove = fix("remove", vec![edit("1:5-1:9", "mut ", "")]);
    assert_eq!(
        plan_with(&[(FILE, text)], &[("js", vec![remove])], &[]),
        skipped("remove", SkipReason::NotCompiled)
    );
}

#[test]
fn skips_fixes_without_edits_or_in_unreadable_files() {
    assert_eq!(
        plan_one("x\n", vec![fix("empty", vec![])]),
        skipped("empty", SkipReason::NoEdits)
    );
    assert_eq!(
        plan_with(
            &[],
            &[("js", vec![fix("gone", vec![edit("1:1-1:2", "x", "y")])])],
            &[(FILE, &["js"])],
        ),
        skipped("gone", SkipReason::Unreadable)
    );
}

#[test]
fn applies_a_fix_proposed_by_several_diagnostics_once() {
    // A file compiled by both the library and its whitebox test check yields
    // the same diagnostic twice; the proposals are a set.
    let remove = fix("remove", vec![edit("1:5-1:9", "mut ", "")]);
    assert_eq!(
        plan_one("let mut x = 1\n", vec![remove.clone(), remove]),
        applied("let x = 1\n")
    );
}

#[test]
fn identifier_occurrence_matches_the_reference_driver() {
    assert!(occurs_as_identifier("a + y", "y"));
    assert!(occurs_as_identifier("y", "y"));
    assert!(!occurs_as_identifier("yy _y y1", "y"));
    // Only ASCII identifier characters delimit, like the reference driver.
    assert!(occurs_as_identifier("éyé", "y"));
    // Overlapping occurrences are all considered.
    assert!(occurs_as_identifier("xa-a-a", "a-a"));
    assert!(!occurs_as_identifier("", "y"));
}

#[test]
fn applies_fixes_that_touch_unless_their_combined_result_joins_tokens() {
    // Adjacent escapes in one literal get one fix each; their edits touch.
    let literal = |title, loc, expect, new_text| {
        let mut e = edit(loc, expect, new_text);
        e.inside_literal = true;
        fix(title, vec![e])
    };
    let escapes = vec![
        literal("first", "1:3-1:9", "\\u{41}", "\\x41"),
        literal("second", "1:9-1:15", "\\u00e9", "\\xC3\\xA9"),
    ];
    assert_eq!(
        plan_one("b\"\\u{41}\\u00e9\"\n", escapes),
        applied("b\"\\x41\\xC3\\xA9\"\n")
    );

    // Removing ` ` and then `-` from `a -b` is fine for each fix, but
    // together they join `a` and `b`.
    let joining = vec![
        fix("first", vec![edit("1:2-1:3", " ", "")]),
        fix("second", vec![edit("1:3-1:4", "-", "")]),
    ];
    assert_eq!(
        plan_one("a -b\n", joining),
        Outcome {
            fixed: vec![(FILE.to_owned(), "a-b\n".to_owned())],
            skipped: vec![("second".to_owned(), SkipReason::ConflictsWithSelectedFix)],
        }
    );
}
