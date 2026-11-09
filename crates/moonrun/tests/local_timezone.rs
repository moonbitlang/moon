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
fn year_queries_preserve_native_results_at_calendar_and_integer_boundaries() {
    use std::fmt::Write;
    use windows_sys::Win32::System::Time::{
        DYNAMIC_TIME_ZONE_INFORMATION, GetDynamicTimeZoneInformation,
        GetTimeZoneInformationForYear, TIME_ZONE_ID_INVALID, TIME_ZONE_INFORMATION,
    };

    let mut zone = DYNAMIC_TIME_ZONE_INFORMATION::default();
    assert_ne!(
        unsafe { GetDynamicTimeZoneInformation(&mut zone) },
        TIME_ZONE_ID_INVALID
    );
    let mut body = String::from(
        "(call $eq (call $capture (i32.const 0)) (i32.const 0))\n\
         (local.set $handle (i64.load (i32.const 0)))\n",
    );
    // Compare both success and failure with Win32: fitting in USHORT does not
    // guarantee that the OS supports a year. Use an unaligned guest buffer.
    for year in [
        1999, 2000, 2038, 2100, 2400, 9999, 10000, 32767, 32768, 65535,
    ] {
        let mut rule = TIME_ZONE_INFORMATION::default();
        let status = if unsafe { GetTimeZoneInformationForYear(year, &zone, &mut rule) } == 0 {
            std::io::Error::last_os_error().raw_os_error().unwrap()
        } else {
            0
        };
        writeln!(
            body,
            "(memory.fill (i32.const 65) (i32.const 165) (i32.const 44))"
        )
        .unwrap();
        writeln!(body, "(call $eq (call $year (local.get $handle) (i32.const {year}) (i32.const 65)) (i32.const {status}))").unwrap();
        let expected: Vec<u8> = if status == 0 {
            [rule.Bias, rule.StandardBias, rule.DaylightBias]
                .into_iter()
                .flat_map(i32::to_le_bytes)
                .chain(
                    [rule.StandardDate, rule.DaylightDate]
                        .into_iter()
                        .flat_map(|date| {
                            [
                                date.wYear,
                                date.wMonth,
                                date.wDayOfWeek,
                                date.wDay,
                                date.wHour,
                                date.wMinute,
                                date.wSecond,
                                date.wMilliseconds,
                            ]
                            .into_iter()
                            .flat_map(u16::to_le_bytes)
                        }),
                )
                .collect()
        } else {
            vec![165; 44]
        };
        for (offset, byte) in expected.into_iter().enumerate() {
            writeln!(
                body,
                "(call $eq (i32.load8_u (i32.const {})) (i32.const {byte}))",
                65 + offset
            )
            .unwrap();
        }
    }
    body.push_str("(call $eq (call $free (local.get $handle)) (i32.const 0))");
    run(&body);
}

#[cfg(windows)]
#[test]
fn unrepresentable_years_are_rejected_without_truncating_or_writing() {
    use std::fmt::Write;
    use windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER;

    let mut body = String::from(
        "(call $eq (call $capture (i32.const 0)) (i32.const 0))\n\
         (local.set $handle (i64.load (i32.const 0)))\n\
         (memory.fill (i32.const 65) (i32.const 165) (i32.const 44))\n",
    );
    // 67561 would become the ordinary year 2025 if truncated to sixteen bits.
    for year in [-1, 65536, 67561, i32::MIN, i32::MAX] {
        writeln!(body, "(call $eq (call $year (local.get $handle) (i32.const {year}) (i32.const 65)) (i32.const {ERROR_INVALID_PARAMETER}))").unwrap();
        for offset in 65..109 {
            writeln!(
                body,
                "(call $eq (i32.load8_u (i32.const {offset})) (i32.const 165))"
            )
            .unwrap();
        }
    }
    body.push_str("(call $eq (call $free (local.get $handle)) (i32.const 0))");
    run(&body);
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
