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

use super::*;

#[test]
fn view_user_rejects_versions_before_contacting_registry() {
    let dir = TestDir::new_empty();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .env("MOON_HOME", dir.as_ref())
        .env("MOONCAKES_REGISTRY", "http://127.0.0.1:1")
        .args(["view", "Bob", "--versions", "--json"])
        .assert().failure().stderr_eq("")
        .stdout_eq(snapbox::str![[r#"
{"version":1,"status":"failure","result":null,"messages":[{"level":"error","message":"--versions requires a module in the form `username/module`"}]}

"#]]);
}

#[test]
fn view_my_requires_saved_login_identity() {
    for (credentials, message) in [
        (
            None,
            "Not logged in. Run `moon login` before using `moon view --my`.",
        ),
        (
            Some(r#"{"token":"  ","username":"Alice"}"#),
            "Not logged in. Run `moon login` before using `moon view --my`.",
        ),
        (
            Some(r#"{"token":"secret"}"#),
            "Username is unavailable. Run `moon login` again before using `moon view --my`.",
        ),
        (
            Some(r#"{"token":"secret","username":"  "}"#),
            "Username is unavailable. Run `moon login` again before using `moon view --my`.",
        ),
    ] {
        let dir = TestDir::new_empty();
        if let Some(credentials) = credentials {
            std::fs::write(dir.join("credentials.json"), credentials).unwrap();
        }
        snapbox::cmd::Command::new(moon_bin())
            .current_dir(&dir)
            .env("MOON_HOME", dir.as_ref())
            .env("MOONCAKES_REGISTRY", "http://127.0.0.1:1")
            .args(["view", "--my", "--quiet"])
            .assert()
            .failure()
            .stdout_eq("")
            .stderr_eq(format!("Error: {message}\n"));
    }
}

#[test]
fn view_json_reports_bootstrap_errors() {
    let dir = TestDir::new_empty();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["-C", "missing", "view", "--my", "--json"])
        .assert().failure().stderr_eq("")
        .stdout_eq(snapbox::str![[r#"
{"version":1,"status":"failure","result":null,"messages":[{"level":"error","message":"[..]missing[..]"}]}

"#]]);
}
