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

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::ported_fns;
use crate::resource::Resource;

ported_fns! {

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "read_console_job_worker"
    )]
    pub(super) fn read(
        file: &Resource,
        len: u32,
        result: &mut Option<Vec<u16>>,
    ) -> AsyncHostResult<i64> {
        #[cfg(windows)]
        {
            use crate::async_sys::internal::event_loop::thread_pool::with_cancellable_region;
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Foundation::{ERROR_BROKEN_PIPE, ERROR_HANDLE_EOF};
            use windows_sys::Win32::System::Console::ReadConsoleW;
            let handle = file.as_file()?.as_raw_handle();
            let mut buffer = vec![0_u16; len as usize];
            let mut count = 0;
            // Capture GetLastError before leaving the cancellation region.
            let count = with_cancellable_region(|| {
                if unsafe {
                    ReadConsoleW(
                        handle,
                        buffer.as_mut_ptr().cast(),
                        len,
                        &mut count,
                        std::ptr::null(),
                    )
                } != 0
                {
                    Ok(count)
                } else {
                    let error = std::io::Error::last_os_error().raw_os_error().unwrap_or(1);
                    if error == ERROR_HANDLE_EOF as i32 || error == ERROR_BROKEN_PIPE as i32 {
                        Ok(0)
                    } else {
                        Err(AsyncHostError::Native(error))
                    }
                }
            })?;
            buffer.truncate(count as usize);
            *result = Some(buffer);
            Ok(i64::from(count))
        }
        #[cfg(not(windows))]
        {
            let _ = (file, len, result);
            Err(AsyncHostError::Native(libc::ENOSYS))
        }
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "write_console_job_worker"
    )]
    pub(super) fn write(file: &Resource, data: &[u16]) -> AsyncHostResult<i64> {
        #[cfg(windows)]
        {
            use crate::async_sys::internal::event_loop::thread_pool::with_cancellable_region;
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::Console::WriteConsoleW;
            let handle = file.as_file()?.as_raw_handle();
            let len = u32::try_from(data.len()).map_err(|_| AsyncHostError::Fault)?;
            let mut count = 0;
            with_cancellable_region(|| {
                if unsafe {
                    WriteConsoleW(
                        handle,
                        data.as_ptr().cast(),
                        len,
                        &mut count,
                        std::ptr::null(),
                    )
                } != 0
                {
                    Ok(i64::from(count))
                } else {
                    Err(AsyncHostError::Native(
                        std::io::Error::last_os_error().raw_os_error().unwrap_or(1),
                    ))
                }
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (file, data);
            Err(AsyncHostError::Native(libc::ENOSYS))
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn windows_console_roundtrips_utf16_through_owned_jobs() {
        // Console attachment is process-global. Keep it out of the test runner
        // and bound the subprocess so an input regression cannot stall CI.
        const CHILD: &str = "MOONRUN_TEST_CONSOLE_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let directory = tempfile::tempdir().unwrap();
            let upstream = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../third_party/moonbitlang_async");
            moon_test_util::test_dir::copy_tree(
                &upstream.join("src"),
                &directory.path().join("src"),
                false,
            )
            .unwrap();
            std::fs::copy(upstream.join("moon.mod"), directory.path().join("moon.mod")).unwrap();
            let package = directory.path().join("src/console_probe");
            std::fs::create_dir(&package).unwrap();
            std::fs::write(
                package.join("main.mbt"),
                include_str!("../../../tests/test_cases/test_async_console.in/main.mbt"),
            )
            .unwrap();
            std::fs::write(
                package.join("moon.pkg"),
                r#"import { "moonbitlang/async", "moonbitlang/async/stdio" }
pkgtype(kind: "executable")
"#,
            )
            .unwrap();
            snapbox::cmd::Command::new("moon")
                .current_dir(directory.path())
                .args(["build", "src/console_probe", "--target", "wasm"])
                .assert()
                .success();
            snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "filesystem::job::console::tests::windows_console_roundtrips_utf16_through_owned_jobs", "--nocapture"])
                .env(CHILD, directory.path().join("_build/wasm/debug/build/console_probe/console_probe.wasm"))
                .timeout(std::time::Duration::from_secs(30))
                .assert().success();
            return;
        }

        use std::sync::Arc;
        use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        };
        use windows_sys::Win32::System::Console::*;
        unsafe {
            FreeConsole();
        }
        assert_ne!(unsafe { AllocConsole() }, 0);
        let input = unsafe {
            CreateFileW(
                windows_sys::w!("CONIN$"),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(input, INVALID_HANDLE_VALUE);
        let input_resource = Arc::new(Resource::new(input));
        assert_ne!(unsafe { SetConsoleMode(input, 0) }, 0);
        let output = unsafe {
            CreateConsoleScreenBuffer(
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                CONSOLE_TEXTMODE_BUFFER,
                std::ptr::null(),
            )
        };
        assert_ne!(output, INVALID_HANDLE_VALUE);
        let output_resource = Arc::new(Resource::new(output));
        assert_ne!(unsafe { SetConsoleMode(output, 0) }, 0);
        assert_ne!(
            unsafe { SetConsoleCursorPosition(output, COORD { X: 0, Y: 0 }) },
            0
        );

        let text = vec![0x03bb_u16, 0x00e9];
        let mut write_job =
            crate::filesystem::Job::write_console(output_resource.clone(), text.clone());
        assert_eq!(write_job.run().unwrap(), 2);
        let mut observed = [0_u16; 2];
        let mut count = 0;
        assert_ne!(
            unsafe {
                ReadConsoleOutputCharacterW(
                    output,
                    observed.as_mut_ptr(),
                    2,
                    COORD { X: 0, Y: 0 },
                    &mut count,
                )
            },
            0
        );
        assert_eq!(count, 2);
        assert_eq!(&observed, text.as_slice());

        let input_events = text
            .iter()
            .map(|&unit| INPUT_RECORD {
                EventType: KEY_EVENT as u16,
                Event: INPUT_RECORD_0 {
                    KeyEvent: KEY_EVENT_RECORD {
                        bKeyDown: 1,
                        wRepeatCount: 1,
                        wVirtualKeyCode: 0,
                        wVirtualScanCode: 0,
                        uChar: KEY_EVENT_RECORD_0 { UnicodeChar: unit },
                        dwControlKeyState: 0,
                    },
                },
            })
            .collect::<Vec<_>>();
        assert_ne!(
            unsafe { WriteConsoleInputW(input, input_events.as_ptr(), 2, &mut count) },
            0
        );
        assert_eq!(count, 2);
        let mut guest = [0xaa_u8; 8];
        let mut received = 0;
        while received < 2 {
            let mut read_job =
                crate::filesystem::Job::read_console(input_resource.clone(), 2 - received);
            let n = read_job.run().unwrap();
            assert!(n > 0);
            read_job
                .copy_read_console_result(0, &mut guest, 0, 1 + received, 2 - received)
                .unwrap();
            received += n as u32;
        }
        assert_eq!(guest, [0xaa, 0xaa, 0xbb, 0x03, 0xe9, 0x00, 0xaa, 0xaa]);

        let mut stat_job = crate::filesystem::Job::fstatx(input_resource.clone(), 1, 16).unwrap();
        assert_eq!(stat_job.run().unwrap(), 0);
        let mut stat = [0_u8; 16];
        stat_job.copy_stat_result(0, &mut stat, 0, 16).unwrap();
        assert_eq!(u64::from_le_bytes(stat[8..16].try_into().unwrap()), 8);

        // Now cross the compiler-generated guest ABI, including stdio setup,
        // UTF-8 conversion, worker dispatch and completion copying.
        assert_ne!(
            unsafe { SetConsoleCursorPosition(output, COORD { X: 0, Y: 0 }) },
            0
        );
        assert_ne!(
            unsafe { WriteConsoleInputW(input, input_events.as_ptr(), 2, &mut count) },
            0
        );
        let stdin = std::fs::File::from(
            input_resource
                .as_handle()
                .unwrap()
                .try_clone_to_owned()
                .unwrap(),
        );
        let stdout = std::fs::File::from(
            output_resource
                .as_handle()
                .unwrap()
                .try_clone_to_owned()
                .unwrap(),
        );
        let stderr = tempfile::tempfile().unwrap();
        let options = crate::RunOptions::default().with_stdio(stdin, stdout, stderr);
        assert_eq!(
            crate::Engine::default()
                .run_file(std::env::var_os(CHILD).unwrap(), options)
                .unwrap(),
            crate::RunOutcome::Completed
        );
        observed.fill(0);
        assert_ne!(
            unsafe {
                ReadConsoleOutputCharacterW(
                    output,
                    observed.as_mut_ptr(),
                    2,
                    COORD { X: 0, Y: 0 },
                    &mut count,
                )
            },
            0
        );
        assert_eq!(count, 2);
        assert_eq!(&observed, text.as_slice());
    }
}
