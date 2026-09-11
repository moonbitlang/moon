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

// Output snapshots for `moon deprecate --dry-run` and `moon view`.
// These exercise the renderers directly with registry responses as fixtures.

use mooncake::registry::{RegistryModuleManifest, RegistryRelease, RegistryUserModules};
use moonutil::cli_support::DeprecateSubcommand;
use serde_json::json;

use super::{
    mooncake_adapter::render_deprecation_preview,
    view::{ViewResult, render_view},
};

#[test]
fn deprecation_preview_lists_every_release_for_both_actions() {
    let versions: Vec<RegistryRelease> = serde_json::from_value(json!([
        {"version": "2.0.0"},
        {"version": "1.5.0-beta.1", "yanked": true, "yanked_reason": "Old reason"},
        {"version": "1.0.0"},
    ]))
    .unwrap();
    for (reason, expected) in [
        (
            Some("Use Owner/replacement"),
            expect_test::expect![[r#"
            Would deprecate Owner/module@2.0.0: Use Owner/replacement
            Would deprecate Owner/module@1.5.0-beta.1: Use Owner/replacement
            Would deprecate Owner/module@1.0.0: Use Owner/replacement
        "#]],
        ),
        (
            None,
            expect_test::expect![[r#"
            Would restore Owner/module@2.0.0
            Would restore Owner/module@1.5.0-beta.1
            Would restore Owner/module@1.0.0
        "#]],
        ),
    ] {
        let command = DeprecateSubcommand {
            module: "Owner/module".to_owned(),
            reason: reason.map(str::to_owned),
            undo: reason.is_none(),
        };
        let mut output = Vec::new();
        render_deprecation_preview(&mut output, &command, &versions).unwrap();
        expected.assert_eq(&String::from_utf8(output).unwrap());
    }
}

#[test]
fn deprecation_preview_handles_an_empty_version_list() {
    let command = DeprecateSubcommand {
        module: "Owner/module".to_owned(),
        reason: None,
        undo: true,
    };
    let mut output = Vec::new();
    render_deprecation_preview(&mut output, &command, &[]).unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "No published versions found for Owner/module.\n"
    );
}

#[test]
fn renders_module_metadata_and_release_deprecation() {
    let manifest: RegistryModuleManifest = serde_json::from_value(serde_json::json!({
        "module": "Alice/tools", "version": "1.10.0", "latest_version": "1.10.0",
        "yanked": false, "yanked_reason": null, "downloads": 42,
        "metadata": {
            "name": "Alice/tools", "version": "1.10.0",
            "description": "Useful\n\u{1b}[31mtools\u{1b}[0m", "license": "Apache-2.0",
            "repository": "https://example.com/tools", "created_at": "2026-09-11T00:00:00Z"
        },
        "versions": [
            {"version": "1.10.0"},
            {"version": "1.9.0", "yanked": true, "yanked_reason": "Use\n\u{1b}[31m1.10.0\u{1b}[0m"},
            {"version": "1.0.0-beta.1", "yanked": true}
        ]
    }))
    .unwrap();
    let releases = manifest.versions.clone();
    let mut output = Vec::new();
    render_view(&mut output, &ViewResult::Module(Box::new(manifest))).unwrap();
    expect_test::expect![[r#"
        Alice/tools@1.10.0
        Description: Useful tools
        License: Apache-2.0
        Repository: https://example.com/tools
        Published: 2026-09-11T00:00:00Z
        Downloads: 42
        Latest: 1.10.0
        Versions: 3 (use --versions to list)
    "#]]
    .assert_eq(&String::from_utf8(output).unwrap());

    let mut output = Vec::new();
    render_view(&mut output, &ViewResult::Versions(releases)).unwrap();
    expect_test::expect![[r#"
        1.10.0
        1.9.0 (deprecated: Use 1.10.0)
        1.0.0-beta.1 (deprecated)
    "#]]
    .assert_eq(&String::from_utf8(output).unwrap());
}

#[test]
fn renders_an_older_release_with_its_own_metadata() {
    let manifest: RegistryModuleManifest = serde_json::from_value(json!({
        "module": "Alice/tools", "version": "1.9.0", "latest_version": "1.10.0",
        "yanked": true, "yanked_reason": "Use 1.10.0",
        "metadata": {
            "name": "Alice/tools", "version": "1.9.0",
            "description": "The previous release of tools",
            "created_at": "2026-08-01T00:00:00Z"
        },
        "versions": [
            {"version": "1.10.0"},
            {"version": "1.9.0", "yanked": true, "yanked_reason": "Use 1.10.0"}
        ]
    }))
    .unwrap();
    let mut output = Vec::new();
    render_view(&mut output, &ViewResult::Module(Box::new(manifest))).unwrap();
    expect_test::expect![[r#"
        Alice/tools@1.9.0 (deprecated: Use 1.10.0)
        Description: The previous release of tools
        Published: 2026-08-01T00:00:00Z
        Latest: 1.10.0
        Versions: 2 (use --versions to list)
    "#]]
    .assert_eq(&String::from_utf8(output).unwrap());
}

#[test]
fn renders_published_modules_and_empty_profiles() {
    let profile: RegistryUserModules = serde_json::from_value(serde_json::json!({
        "username": "Alice", "modules": [
            {"name": "Alice/tools", "version": "1.0.0", "yanked": true, "yanked_reason": "Use Alice/tools2"},
            {"name": "Alice/tools2", "version": "2.0.0"}
        ]
    })).unwrap();
    let mut output = Vec::new();
    render_view(&mut output, &ViewResult::Published(profile)).unwrap();
    expect_test::expect![[r#"
        Alice/tools@1.0.0 (deprecated: Use Alice/tools2)
        Alice/tools2@2.0.0
    "#]]
    .assert_eq(&String::from_utf8(output).unwrap());

    let mut output = Vec::new();
    render_view(
        &mut output,
        &ViewResult::Published(RegistryUserModules {
            username: "Alice".to_owned(),
            modules: Vec::new(),
        }),
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "No published modules.\n"
    );
}

#[test]
fn rejects_unsafe_names_before_rendering_published_modules() {
    let profile: RegistryUserModules = serde_json::from_value(serde_json::json!({
        "username": "Alice", "modules": [
            {"name": "Alice/tools", "version": "1.0.0"},
            {"name": "Alice/z\nspoofed", "version": "1.0.0"}
        ]
    }))
    .unwrap();
    let mut output = Vec::new();
    let error = render_view(&mut output, &ViewResult::Published(profile)).unwrap_err();
    assert_eq!(
        error.to_string(),
        "registry user response contains an invalid module name"
    );
    assert!(output.is_empty());
}
