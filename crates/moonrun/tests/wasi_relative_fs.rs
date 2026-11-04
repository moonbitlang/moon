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

//! Exercise the WASI adapter and shared handle-relative filesystem operations.

use moonrun::{Engine, RunOptions, RunOutcome, WorkingDirectory};

#[test]
fn open_and_removal_keep_the_directory_descriptor_after_rename() {
    const RIGHT_FD_READ: u64 = 1 << 1;
    const RIGHT_PATH_OPEN: u64 = 1 << 13;
    const RIGHT_PATH_REMOVE_DIRECTORY: u64 = 1 << 25;
    const RIGHT_PATH_UNLINK_FILE: u64 = 1 << 26;
    const OFLAGS_DIRECTORY: i32 = 2;
    let engine = Engine::default();
    let module = engine.compile("relative-wasi.wasm", wat::parse_str(format!(r#"(module
        (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "path_rename" (func $rename (param i32 i32 i32 i32 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "path_unlink_file" (func $unlink (param i32 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "path_remove_directory" (func $rmdir (param i32 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "originalmovedreplacementfileempty")
        (func $ok (param $errno i32) local.get $errno if unreachable end)
        (func (export "_start") (local $dir i32) (local $file i32)
            i32.const 3 i32.const 1 i32.const 0 i32.const 8 i32.const {directory}
            i64.const {directory_rights} i64.const {read} i32.const 0 i32.const 64
            call $open call $ok
            i32.const 64 i32.load local.set $dir
            ;; Rename both entries after acquiring the original directory.
            i32.const 3 i32.const 0 i32.const 8 i32.const 3 i32.const 8 i32.const 5 call $rename call $ok
            i32.const 3 i32.const 13 i32.const 11 i32.const 3 i32.const 0 i32.const 8 call $rename call $ok
            local.get $dir i32.const 1 i32.const 24 i32.const 4 i32.const 0
            i64.const {read} i64.const 0 i32.const 0 i32.const 68 call $open call $ok
            i32.const 68 i32.load local.set $file
            i32.const 80 i32.const 96 i32.store
            i32.const 84 i32.const 1 i32.store
            local.get $file i32.const 80 i32.const 1 i32.const 88 call $read call $ok
            i32.const 88 i32.load i32.const 1 i32.ne if unreachable end
            i32.const 96 i32.load8_u i32.const 107 i32.ne if unreachable end ;; 'k', not 'f'
            local.get $file call $close call $ok
            ;; The two removal imports must reject the opposite entry kind.
            local.get $dir i32.const 24 i32.const 4 call $rmdir i32.eqz if unreachable end
            local.get $dir i32.const 28 i32.const 5 call $unlink i32.eqz if unreachable end
            local.get $dir i32.const 24 i32.const 4 call $unlink call $ok
            local.get $dir i32.const 28 i32.const 5 call $rmdir call $ok
            local.get $dir call $close call $ok))"#,
        directory = OFLAGS_DIRECTORY,
        directory_rights = RIGHT_PATH_OPEN | RIGHT_PATH_UNLINK_FILE | RIGHT_PATH_REMOVE_DIRECTORY,
        read = RIGHT_FD_READ,
    )).unwrap()).unwrap();

    for restricted in [false, true] {
        let root = tempfile::tempdir().unwrap();
        for (name, contents) in [("original", b"keep"), ("replacement", b"fake")] {
            std::fs::create_dir_all(root.path().join(name).join("empty")).unwrap();
            std::fs::write(root.path().join(name).join("file"), contents).unwrap();
        }
        let mut options = RunOptions::default()
            .with_working_directory(WorkingDirectory::Fixed(root.path().to_owned()));
        if restricted {
            let policy = root.path().join("policy.toml");
            std::fs::write(&policy, "[fs]\nread = [\".\"]\nwrite = [\".\"]\n").unwrap();
            options = options.with_policy_file(policy).unwrap();
        }
        assert_eq!(engine.run(&module, options).unwrap(), RunOutcome::Completed);
        assert_eq!(
            std::fs::read(root.path().join("original/file")).unwrap(),
            b"fake"
        );
        assert!(root.path().join("original/empty").is_dir());
        assert!(!root.path().join("moved/file").exists());
        assert!(!root.path().join("moved/empty").exists());
    }
}
