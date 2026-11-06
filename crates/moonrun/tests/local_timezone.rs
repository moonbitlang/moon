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

use moonrun::{Engine, RunOptions, RunOutcome};

const IMPORTS: &str = r#"
    (import "moonbitlang/core" "time/windows/get_dynamic_time_zone_information" (func $capture (param i32) (result i32)))
    (import "moonbitlang/core" "time/windows/get_time_zone_metadata" (func $meta (param i64 i32 i32) (result i32)))
    (import "moonbitlang/core" "time/windows/get_current_time_zone_rule" (func $current (param i64 i32) (result i32)))
    (import "moonbitlang/core" "time/windows/get_dynamic_time_zone_information_effective_years" (func $years (param i64 i32) (result i32)))
    (import "moonbitlang/core" "time/windows/get_time_zone_information_for_year" (func $year (param i64 i32 i32) (result i32)))
    (import "moonbitlang/core" "time/windows/free_time_zone" (func $free (param i64) (result i32)))
"#;

fn run(body: &str) {
    let source = format!(
        r#"(module {IMPORTS}
        (memory (export "memory") 1)
        (func $eq (param i32 i32) local.get 0 local.get 1 i32.ne if unreachable end)
        (func (export "_start") (local $handle i64) (local $status i32) {body}))"#
    );
    let engine = Engine::default();
    let module = engine
        .compile("timezone.wasm", wat::parse_str(source).unwrap())
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let policy = dir.path().join("policy.json");
    std::fs::write(&policy, "{}").unwrap();
    let options = RunOptions::default().with_policy_file(policy).unwrap();
    assert_eq!(engine.run(&module, options).unwrap(), RunOutcome::Completed);
}

#[cfg(unix)]
#[test]
fn windows_imports_link_on_unix_and_return_native_not_supported() {
    let code = libc::ENOTSUP;
    run(&format!(
        r#"
        (i32.store (i32.const 0) (i32.const 123))
        (call $eq (call $capture (i32.const 0)) (i32.const {code}))
        (call $eq (call $meta (i64.const 0) (i32.const 0) (i32.const 264)) (i32.const {code}))
        (call $eq (call $current (i64.const 0) (i32.const 0)) (i32.const {code}))
        (call $eq (call $years (i64.const 0) (i32.const 0)) (i32.const {code}))
        (call $eq (call $year (i64.const 0) (i32.const 2025) (i32.const 0)) (i32.const {code}))
        (call $eq (call $free (i64.const 0)) (i32.const {code}))
        (call $eq (i32.load (i32.const 0)) (i32.const 123))
    "#
    ));
}

#[cfg(windows)]
#[test]
fn windows_capture_and_queries_preserve_native_results_under_empty_policy() {
    use windows_sys::Win32::System::Time::{
        DYNAMIC_TIME_ZONE_INFORMATION, GetDynamicTimeZoneInformation,
        GetDynamicTimeZoneInformationEffectiveYears, GetTimeZoneInformationForYear,
        TIME_ZONE_ID_INVALID, TIME_ZONE_INFORMATION,
    };
    let mut zone = DYNAMIC_TIME_ZONE_INFORMATION::default();
    assert_ne!(
        unsafe { GetDynamicTimeZoneInformation(&mut zone) },
        TIME_ZONE_ID_INVALID
    );
    let (mut first, mut last) = (0, 0);
    let years_status =
        unsafe { GetDynamicTimeZoneInformationEffectiveYears(&zone, &mut first, &mut last) };
    let mut rule = TIME_ZONE_INFORMATION::default();
    let year_status = if unsafe { GetTimeZoneInformationForYear(2025, &zone, &mut rule) } == 0 {
        std::io::Error::last_os_error().raw_os_error().unwrap()
    } else {
        0
    };
    let name_bytes = zone
        .TimeZoneKeyName
        .iter()
        .position(|&u| u == 0)
        .unwrap_or(zone.TimeZoneKeyName.len())
        * 2;
    run(&format!(
        r#"
        (call $eq (call $capture (i32.const 1)) (i32.const 0))
        (local.set $handle (i64.load (i32.const 1)))
        (call $eq (call $current (local.get $handle) (i32.const 11)) (i32.const 0))
        (call $eq (i32.load (i32.const 11)) (i32.const {bias}))
        (i32.store8 (i32.const {after_name}) (i32.const 123))
        (call $eq (call $meta (local.get $handle) (i32.const 61) (i32.const 264)) (i32.const 0))
        (call $eq (i32.load (i32.const 61)) (i32.const {disabled}))
        (call $eq (i32.load (i32.const 65)) (i32.const {name_bytes}))
        (call $eq (i32.load8_u (i32.const {after_name})) (i32.const 123))
        (if (i32.const {name_bytes}) (then
            (call $eq (i32.load16_u (i32.const 69)) (i32.const {name}))))
        ;; Editing the guest's key name cannot retarget the capture.
        (i32.store (i32.const 69) (i32.const -1))
        (local.set $status (call $years (local.get $handle) (i32.const 401)))
        (call $eq (local.get $status) (i32.const {years_status}))
        (if (i32.eqz (local.get $status)) (then
            (call $eq (i32.load (i32.const 401)) (i32.const {first}))
            (call $eq (i32.load (i32.const 405)) (i32.const {last}))))
        (local.set $status (call $year (local.get $handle) (i32.const 2025) (i32.const 411)))
        (call $eq (local.get $status) (i32.const {year_status}))
        (if (i32.eqz (local.get $status)) (then
            (call $eq (i32.load (i32.const 411)) (i32.const {rule_bias}))
            (call $eq (i32.load16_u (i32.const 441)) (i32.const {daylight_month}))))
        (call $eq (call $free (local.get $handle)) (i32.const 0))
    "#,
        bias = zone.Bias,
        disabled = u32::from(zone.DynamicDaylightTimeDisabled),
        name = zone.TimeZoneKeyName[0],
        after_name = 69 + name_bytes,
        rule_bias = rule.Bias,
        daylight_month = rule.DaylightDate.wMonth
    ));
}

#[cfg(windows)]
#[test]
fn invalid_ranges_and_retired_handles_do_not_write_results() {
    use windows_sys::Win32::Foundation::{
        ERROR_INSUFFICIENT_BUFFER, ERROR_INVALID_ADDRESS, ERROR_INVALID_HANDLE,
        ERROR_INVALID_PARAMETER,
    };
    run(&format!(
        r#"
        (call $eq (call $capture (i32.const 65535)) (i32.const {fault}))
        ;; Only the handle is written: the final eight bytes suffice.
        (call $eq (call $capture (i32.const 65528)) (i32.const 0))
        (local.set $handle (i64.load (i32.const 65528)))
        (call $eq (call $years (local.get $handle) (i32.const -1)) (i32.const {fault}))
        (i32.store (i32.const 512) (i32.const 123))
        (call $eq (call $meta (local.get $handle) (i32.const 512) (i32.const 4)) (i32.const {too_small}))
        (call $eq (call $current (local.get $handle) (i32.const 65535)) (i32.const {fault}))
        (call $eq (call $year (local.get $handle) (i32.const 65536) (i32.const 512)) (i32.const {invalid}))
        (call $eq (call $free (local.get $handle)) (i32.const 0))
        (call $eq (call $capture (i32.const 0)) (i32.const 0))
        (call $eq (call $years (local.get $handle) (i32.const 512)) (i32.const {badf}))
        (call $eq (call $current (local.get $handle) (i32.const 512)) (i32.const {badf}))
        (call $eq (call $meta (local.get $handle) (i32.const 512) (i32.const 264)) (i32.const {badf}))
        (call $eq (call $free (local.get $handle)) (i32.const {badf}))
        (call $eq (i32.load (i32.const 512)) (i32.const 123))
        (call $eq (call $free (i64.load (i32.const 0))) (i32.const 0))
    "#,
        fault = ERROR_INVALID_ADDRESS,
        invalid = ERROR_INVALID_PARAMETER,
        too_small = ERROR_INSUFFICIENT_BUFFER,
        badf = ERROR_INVALID_HANDLE
    ));
}
