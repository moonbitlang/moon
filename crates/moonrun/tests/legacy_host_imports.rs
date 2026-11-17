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

#[cfg(feature = "v8")]
#[path = "common/test_init.rs"]
mod test_init;

use moonrun::{Engine, RunOptions, RunOutcome};

#[test]
fn legacy_async_filesystem_jobs_preserve_guest_abi_and_behavior() {
    let engine = Engine::default();
    // Freeze both pre-openat guest interfaces independently of the current
    // async wrapper. Rebuilding current MoonBit sources cannot check this ABI.
    for (name, extra_params, stat_args, result_imports, check_kind) in [
        (
            "make_open_job",
            "",
            "",
            r#"(import "moonbitlang/async" "thread_pool/open_job_get_kind" (func $kind (param i64) (result i32)))
                (import "moonbitlang/async" "thread_pool/open_job_get_dev_id" (func $device (param i64) (result i64)))
                (import "moonbitlang/async" "thread_pool/open_job_get_file_id" (func $identity (param i64) (result i64)))"#,
            r#"(if (i32.ne (call $kind (local.get $job)) (local.get $kind)) (then unreachable))
                (drop (call $device (local.get $job)))
                (drop (call $identity (local.get $job)))"#,
        ),
        (
            "make_open_stat_job",
            "i32 i32",
            "(i32.const 13) (i32.const 32)",
            r#"(import "moonbitlang/async" "thread_pool/get_stat_result" (func $stat (param i64 i32 i32)))"#,
            r#"(call $stat (local.get $job) (i32.const 64) (i32.const 32))
                (if (i32.ne (i32.load (i32.const 64)) (i32.const 32)) (then unreachable))
                (if (i32.ne (i32.load (i32.const 68)) (i32.const 13)) (then unreachable))
                (if (i32.ne (i32.load (i32.const 72)) (local.get $kind)) (then unreachable))"#,
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("dir")).unwrap();
        std::fs::write(root.path().join("dir/entry"), []).unwrap();
        std::fs::create_dir(root.path().join("empty")).unwrap();
        // The old remove import uses remove(3) on Unix, but DeleteFileW on
        // Windows. It must not silently acquire removeat's directory semantics.
        let remove_directory_error_check = if cfg!(windows) { "i32.eqz" } else { "" };
        let wasm = wat::parse_str(format!(
            r#"(module
                (import "moonbitlang/async" "thread_pool/{name}"
                    (func $open (param i32 i32 i32 i32 i32 i32 i32 {extra_params}) (result i64)))
                {result_imports}
                (import "moonbitlang/async" "thread_pool/open_job_get_fd" (func $fd (param i64) (result i64)))
                (import "moonbitlang/async" "thread_pool/make_read_job" (func $read (param i64 i32 i64) (result i64)))
                (import "moonbitlang/async" "thread_pool/get_read_result" (func $read_result (param i64 i32 i32 i32)))
                (import "moonbitlang/async" "thread_pool/make_write_job" (func $write (param i64 i32 i32 i32 i64) (result i64)))
                (import "moonbitlang/async" "thread_pool/make_readdir_job" (func $readdir (param i64 i64 i32 i32) (result i64)))
                (import "moonbitlang/async" "thread_pool/make_remove_job" (func $remove (param i32 i32) (result i64)))
                (import "moonbitlang/async" "thread_pool/run_job" (func $run (param i64)))
                (import "moonbitlang/async" "thread_pool/job_get_err" (func $error (param i64) (result i32)))
                (import "moonbitlang/async" "thread_pool/job_get_ret" (func $ret (param i64) (result i32)))
                (import "moonbitlang/async" "thread_pool/free_job" (func $free (param i64)))
                (import "moonbitlang/async" "fd_util/close_fd" (func $close (param i64) (result i32)))
                (import "moonbitlang/async" "c_buffer/new" (func $buffer (param i32) (result i64)))
                (import "moonbitlang/async" "c_buffer/free" (func $free_buffer (param i64)))
                (memory (export "memory") 1)
                (data (i32.const 0) "f\00i\00l\00e\00")
                (data (i32.const 16) "d\00i\00r\00")
                (data (i32.const 32) "e\00m\00p\00t\00y\00")
                (data (i32.const 48) "old wasm")
                (func $checked (param $job i64) (result i64)
                    (call $run (local.get $job))
                    (if (call $error (local.get $job)) (then unreachable))
                    (local.get $job))
                (func $opened (param $path i32) (param $len i32) (param $access i32)
                    (param $create i32) (param $kind i32) (result i64) (local $job i64) (local $file i64)
                    (local.set $job (call $checked (call $open (local.get $path) (local.get $len)
                        (local.get $access) (local.get $create) (i32.const 0) (i32.const 0) (i32.const 384) {stat_args})))
                    {check_kind}
                    (local.set $file (call $fd (local.get $job)))
                    (call $free (local.get $job))
                    (local.get $file))
                (func $list (param $access i32) (local $file i64) (local $buf i64) (local $job i64)
                    (local.set $file (call $opened (i32.const 16) (i32.const 3) (local.get $access) (i32.const 0) (i32.const 2)))
                    (local.set $buf (call $buffer (i32.const 4096)))
                    (local.set $job (call $checked (call $readdir (local.get $file) (local.get $buf) (i32.const 4096) (i32.const 1))))
                    (if (i32.le_s (call $ret (local.get $job)) (i32.const 0)) (then unreachable))
                    (call $free (local.get $job))
                    (call $free_buffer (local.get $buf))
                    (if (call $close (local.get $file)) (then unreachable)))
                (func (export "_start") (local $file i64) (local $job i64)
                    (local.set $file (call $opened (i32.const 0) (i32.const 4) (i32.const 2) (i32.const 3) (i32.const 1)))
                    (local.set $job (call $checked (call $write (local.get $file) (i32.const 48) (i32.const 0) (i32.const 8) (i64.const -1))))
                    (if (i32.ne (call $ret (local.get $job)) (i32.const 8)) (then unreachable))
                    (call $free (local.get $job))
                    (if (call $close (local.get $file)) (then unreachable))
                    (local.set $file (call $opened (i32.const 0) (i32.const 4) (i32.const 0) (i32.const 0) (i32.const 1)))
                    (local.set $job (call $checked (call $read (local.get $file) (i32.const 8) (i64.const -1))))
                    (if (i32.ne (call $ret (local.get $job)) (i32.const 8)) (then unreachable))
                    (call $read_result (local.get $job) (i32.const 128) (i32.const 0) (i32.const 8))
                    (if (i64.ne (i64.load (i32.const 128)) (i64.load (i32.const 48))) (then unreachable))
                    (call $free (local.get $job))
                    (if (call $close (local.get $file)) (then unreachable))
                    ;; Older opendir used ReadOnly; watchers open with ListDirectory.
                    (call $list (i32.const 0))
                    (call $list (i32.const 3))
                    (call $free (call $checked (call $remove (i32.const 0) (i32.const 4))))
                    (local.set $job (call $remove (i32.const 32) (i32.const 5)))
                    (call $run (local.get $job))
                    (call $error (local.get $job)) {remove_directory_error_check} if unreachable end
                    (call $free (local.get $job))))"#,
        ))
        .unwrap();
        let module = engine.compile(format!("legacy-{name}.wasm"), wasm).unwrap();
        assert_eq!(
            engine
                .run(
                    &module,
                    RunOptions::default().with_working_directory(moonrun::WorkingDirectory::Fixed(
                        root.path().to_owned(),
                    )),
                )
                .unwrap(),
            RunOutcome::Completed,
            "{name}"
        );
        assert!(!root.path().join("file").exists(), "{name}");
        assert_eq!(root.path().join("empty").is_dir(), cfg!(windows), "{name}");
        assert!(root.path().join("dir/entry").is_file(), "{name}");
    }
}

#[test]
fn byte_builders_snapshot_and_readers_have_independent_positions() {
    let engine = Engine::default();
    let module = engine.compile("bytes.wasm", wat::parse_str(r#"(module
        (import "__moonbit_fs_unstable" "begin_create_byte_array" (func $builder (result externref)))
        (import "__moonbit_fs_unstable" "byte_array_append_byte" (func $append (param externref i32)))
        (import "__moonbit_fs_unstable" "finish_create_byte_array" (func $finish (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "begin_read_byte_array" (func $reader (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "byte_array_read_byte" (func $read (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "finish_read_byte_array" (func $release (param externref)))
        (func (export "_start")
            (local $builder externref) (local $first externref) (local $second externref)
            call $builder local.set $builder
            local.get $builder i32.const 256 call $append
            local.get $builder i32.const -1 call $append
            local.get $builder call $finish
            call $reader local.set $first
            local.get $builder i32.const 42 call $append
            local.get $builder call $finish
            call $reader local.set $second
            local.get $first call $read i32.const 0 i32.ne if unreachable end
            local.get $second call $read i32.const 0 i32.ne if unreachable end
            local.get $first call $read i32.const 255 i32.ne if unreachable end
            local.get $first call $read i32.const -1 i32.ne if unreachable end
            local.get $first call $release
            local.get $second call $read i32.const 255 i32.ne if unreachable end
            local.get $second call $read i32.const 42 i32.ne if unreachable end
            local.get $second call $read i32.const -1 i32.ne if unreachable end
            local.get $second call $release))"#).unwrap()).unwrap();
    assert_eq!(
        engine.run(&module, RunOptions::default()).unwrap(),
        RunOutcome::Completed
    );
}

#[test]
fn legacy_arguments_preserve_unicode_and_reader_positions() {
    let engine = Engine::default();
    let module = engine.compile("legacy-args.wasm", wat::parse_str(r#"(module
        (import "__moonbit_fs_unstable" "args_get" (func $args (result externref)))
        (import "__moonbit_fs_unstable" "array_len" (func $len (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "array_get" (func $get (param externref i32) (result externref)))
        (import "__moonbit_fs_unstable" "jsvalue_is_string" (func $is_string (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "begin_read_string" (func $reader (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "string_read_char" (func $read (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "begin_read_string_array" (func $array_reader (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "string_array_read_string" (func $array_read (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "finish_read_string_array" (func $done (param externref)))
        (func (export "_start") (local $args externref) (local $r externref) (local $ar externref)
            call $args local.tee $args call $len i32.const 2 i32.ne if unreachable end
            local.get $args call $is_string if unreachable end
            ref.null extern call $is_string if unreachable end
            local.get $args i32.const 1 call $get
            call $reader local.set $r
            local.get $r call $read i32.const 955 i32.ne if unreachable end
            local.get $r call $read i32.const 0xd83e i32.ne if unreachable end
            local.get $r call $read i32.const 0xdd80 i32.ne if unreachable end
            local.get $r call $read i32.const -1 i32.ne if unreachable end
            local.get $args call $array_reader local.set $ar
            local.get $ar call $array_read call $reader call $read
            i32.const 108 i32.ne if unreachable end
            local.get $ar call $array_read call $reader call $read
            i32.const 955 i32.ne if unreachable end
            local.get $ar call $array_read call $reader call $read
            i32.const 102 i32.ne if unreachable end
            local.get $ar call $done))"#).unwrap()).unwrap();
    for _ in 0..2 {
        assert_eq!(
            engine
                .run(&module, RunOptions::default().with_args(["λ🦀"]))
                .unwrap(),
            RunOutcome::Completed
        );
    }
}

const STRING_HELPERS: &str = r#"
    (import "__moonbit_fs_unstable" "begin_create_string" (func $builder (result externref)))
    (import "__moonbit_fs_unstable" "string_append_char" (func $append (param externref i32)))
    (import "__moonbit_fs_unstable" "finish_create_string" (func $finish (param externref) (result externref)))
    (import "__moonbit_fs_unstable" "begin_read_string" (func $reader (param externref) (result externref)))
    (import "__moonbit_fs_unstable" "string_read_char" (func $read (param externref) (result i32)))
    (memory (export "memory") 1)
    (func $text (param $offset i32) (param $length i32) (result externref)
        (local $b externref)
        call $builder local.set $b
        block $done loop $next
            local.get $length i32.eqz br_if $done
            local.get $b local.get $offset i32.load8_u call $append
            local.get $offset i32.const 1 i32.add local.set $offset
            local.get $length i32.const 1 i32.sub local.set $length
            br $next
        end end
        local.get $b call $finish)
"#;

#[test]
fn filesystem_uses_fixed_working_directory_and_retains_status_results() {
    let engine = Engine::default();
    let module = engine.compile("filesystem.wasm", wat::parse_str(format!(r#"(module
        (import "__moonbit_fs_unstable" "write_string_to_file" (func $write (param externref externref)))
        (import "__moonbit_fs_unstable" "read_file_to_string" (func $load (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "read_file_to_bytes_new" (func $load_bytes (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "get_file_content" (func $contents (result externref)))
        (import "__moonbit_fs_unstable" "begin_read_byte_array" (func $bytes (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "byte_array_read_byte" (func $byte (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "is_file_new" (func $is_file (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "remove_file_new" (func $remove (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "get_error_message" (func $error (result externref)))
        {STRING_HELPERS}
        (data (i32.const 0) "output.txtOKmissing")
        (func (export "_start") (local $path externref) (local $r externref)
            i32.const 0 i32.const 10 call $text local.set $path
            local.get $path i32.const 10 i32.const 2 call $text call $write
            local.get $path call $is_file i32.const 1 i32.ne if unreachable end
            local.get $path call $load call $reader local.set $r
            local.get $r call $read i32.const 79 i32.ne if unreachable end
            local.get $r call $read i32.const 75 i32.ne if unreachable end
            local.get $path call $load_bytes if unreachable end
            i32.const 12 i32.const 7 call $text call $load_bytes
            i32.const -1 i32.ne if unreachable end
            call $error call $reader call $read i32.const -1 i32.eq if unreachable end
            call $contents call $bytes local.set $r
            local.get $r call $byte i32.const 79 i32.ne if unreachable end
            local.get $r call $byte i32.const 75 i32.ne if unreachable end
            local.get $r call $byte i32.const -1 i32.ne if unreachable end
            local.get $path call $remove if unreachable end))"#)).unwrap()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    assert_eq!(
        engine
            .run(
                &module,
                RunOptions::default().with_working_directory(moonrun::WorkingDirectory::Fixed(
                    directory.path().to_owned()
                ))
            )
            .unwrap(),
        RunOutcome::Completed
    );
    assert!(!directory.path().join("output.txt").exists());
}

#[test]
fn legacy_stdio_random_and_time_work_without_guest_memory() {
    let engine = Engine::default();
    let module = engine.compile("services.wasm", wat::parse_str(r#"(module
        (import "spectest" "read_char" (func $read (result i32)))
        (import "__moonbit_io_unstable" "write_char" (func $write (param i32 i32)))
        (import "__moonbit_io_unstable" "flush" (func $flush (param i32)))
        (import "__moonbit_rand_unstable" "stdrng_seed_from_u64" (func $rng (param i32) (result externref)))
        (import "__moonbit_rand_unstable" "stdrng_gen_range" (func $random (param externref i32) (result i32)))
        (import "__moonbit_time_unstable" "instant_now" (func $instant (result externref)))
        (import "__moonbit_time_unstable" "instant_elapsed_as_secs_f64" (func $elapsed (param externref) (result f64)))
        (import "__moonbit_time_unstable" "now" (func $now (result i64)))
        (func (export "_start") (local $a externref) (local $b externref)
            i32.const 1 call $read call $write
            i32.const 2 call $read call $write
            call $read i32.const -1 i32.ne if unreachable end
            i32.const 1 call $flush i32.const 2 call $flush
            i32.const -1 call $rng local.set $a
            i32.const -1 call $rng local.set $b
            local.get $a i32.const 100000 call $random
            local.get $b i32.const 100000 call $random i32.ne if unreachable end
            local.get $a i32.const 1 call $random if unreachable end
            call $instant call $elapsed f64.const 0 f64.lt if unreachable end
            call $now i64.const 0 i64.le_s if unreachable end))"#).unwrap()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("in"), "λ🦀").unwrap();
    let options = RunOptions::default().with_stdio(
        std::fs::File::open(directory.path().join("in")).unwrap(),
        std::fs::File::create(directory.path().join("out")).unwrap(),
        std::fs::File::create(directory.path().join("err")).unwrap(),
    );
    assert_eq!(engine.run(&module, options).unwrap(), RunOutcome::Completed);
    assert_eq!(
        std::fs::read_to_string(directory.path().join("out")).unwrap(),
        "λ"
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("err")).unwrap(),
        "🦀"
    );
}

#[test]
fn environment_mutations_are_owned_by_each_run() {
    let engine = Engine::default();
    let module = engine.compile("environment.wasm", wat::parse_str(format!(r#"(module
        (import "__moonbit_fs_unstable" "get_env_vars" (func $entries (result externref)))
        (import "__moonbit_fs_unstable" "array_len" (func $len (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "array_get" (func $get (param externref i32) (result externref)))
        (import "__moonbit_fs_unstable" "set_env_var" (func $set (param externref externref)))
        (import "__moonbit_fs_unstable" "unset_env_var" (func $unset (param externref)))
        (import "__moonbit_fs_unstable" "env_get_var" (func $lookup (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "get_env_var_exists" (func $exists (param externref) (result i32)))
        {STRING_HELPERS}
        (data (i32.const 0) "HOST_IMPORT_TESTnew")
        (func (export "_start") (local $key externref) (local $entries externref)
            i32.const 0 i32.const 16 call $text local.set $key
            local.get $key call $lookup call $reader call $read
            i32.const 955 i32.ne if unreachable end
            local.get $key i32.const 16 i32.const 3 call $text call $set
            call $entries local.tee $entries call $len i32.const 2 i32.ne if unreachable end
            local.get $entries i32.const 0 call $get call $reader call $read
            i32.const 72 i32.ne if unreachable end
            local.get $entries i32.const 1 call $get call $reader call $read
            i32.const 110 i32.ne if unreachable end
            local.get $key call $unset
            local.get $key call $exists if unreachable end
            call $entries call $len if unreachable end))"#)).unwrap()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let policy = directory.path().join("environment.toml");
    // Explicit inputs and policy provisioning both create an owned environment.
    std::fs::write(&policy, "[env.set]\nHOST_IMPORT_TEST = \"λ🦀\"\n").unwrap();
    let ambient = std::env::var_os("HOST_IMPORT_TEST");
    for _ in 0..2 {
        for options in [
            RunOptions::default().with_environment([("HOST_IMPORT_TEST", "λ🦀")]),
            RunOptions::default().with_policy_file(&policy).unwrap(),
            RunOptions::default()
                .with_environment([("HOST_IMPORT_TEST", "overridden"), ("EXTRA", "removed")])
                .with_policy_file(&policy)
                .unwrap(),
        ] {
            assert_eq!(engine.run(&module, options).unwrap(), RunOutcome::Completed);
        }
    }
    assert_eq!(std::env::var_os("HOST_IMPORT_TEST"), ambient);
}

#[test]
fn explicit_empty_environment_does_not_inherit_host_entries() {
    let engine = Engine::default();
    let module = engine
        .compile(
            "empty-environment.wasm",
            wat::parse_str(
                r#"(module
                (import "__moonbit_fs_unstable" "get_env_vars" (func $entries (result externref)))
                (import "__moonbit_fs_unstable" "array_len" (func $len (param externref) (result i32)))
                (func (export "_start") call $entries call $len if unreachable end))"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        engine
            .run(
                &module,
                RunOptions::default().with_environment([] as [(&str, &str); 0]),
            )
            .unwrap(),
        RunOutcome::Completed
    );
}

#[test]
fn filesystem_denial_precedes_invalid_byte_payload() {
    let wasm = wat::parse_str(format!(r#"(module
        (import "__moonbit_fs_unstable" "write_bytes_to_file_new" (func $write (param externref externref) (result i32)))
        (import "__moonbit_fs_unstable" "get_error_message" (func $error (result externref)))
        (import "__moonbit_io_unstable" "write_char" (func $write_char (param i32 i32)))
        {STRING_HELPERS}
        (data (i32.const 0) "denied.bin")
        (func (export "_start") (local $reader externref) (local $char i32)
            i32.const 0 i32.const 10 call $text ref.null extern call $write
            i32.const -1 i32.ne if unreachable end
            call $error call $reader local.set $reader
            block $done loop $next
                local.get $reader call $read local.tee $char i32.const -1 i32.eq br_if $done
                i32.const 1 local.get $char call $write_char br $next
            end end
            i32.const 1 i32.const 10 call $write_char))"#)).unwrap();
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("main.wasm"), wasm).unwrap();
    std::fs::write(
        directory.path().join("policy.toml"),
        "[fs]\nread = []\nwrite = []\n",
    )
    .unwrap();
    snapbox::cmd::Command::new(env!("CARGO_BIN_EXE_moonrun"))
        .current_dir(directory.path())
        .args(["--policy", "policy.toml", "main.wasm"])
        .assert()
        .success()
        .stdout_eq("Permission denied: denied.bin\n")
        .stderr_eq("Sandbox policy blocked file write: \"denied.bin\"\n");
    assert!(!directory.path().join("denied.bin").exists());
}

#[test]
fn directory_and_byte_file_operations_share_results() {
    let engine = Engine::default();
    let module = engine.compile("directories.wasm", wat::parse_str(format!(r#"(module
        (import "__moonbit_fs_unstable" "create_dir" (func $mkdir (param externref)))
        (import "__moonbit_fs_unstable" "create_dir_new" (func $mkdir_new (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "remove_dir" (func $rmdir (param externref)))
        (import "__moonbit_fs_unstable" "remove_dir_new" (func $rmdir_new (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "is_dir" (func $is_dir (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "is_dir_new" (func $is_dir_new (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "path_exists" (func $exists (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "read_dir" (func $ls (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "read_dir_new" (func $ls_new (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "get_dir_files" (func $entries (result externref)))
        (import "__moonbit_fs_unstable" "array_len" (func $len (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "array_get" (func $get (param externref i32) (result externref)))
        (import "__moonbit_fs_unstable" "current_dir" (func $cwd (result externref)))
        (import "__moonbit_io_unstable" "read_bytes_from_stdin" (func $stdin (result externref)))
        (import "__moonbit_fs_unstable" "write_bytes_to_file" (func $write (param externref externref)))
        (import "__moonbit_fs_unstable" "write_bytes_to_file_new" (func $write_new (param externref externref) (result i32)))
        (import "__moonbit_fs_unstable" "is_file" (func $is_file (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "remove_file" (func $unlink (param externref)))
        {STRING_HELPERS}
        (data (i32.const 0) "dirdir/filecopy")
        (func (export "_start") (local $dir externref) (local $file externref) (local $bytes externref)
            i32.const 0 i32.const 3 call $text local.set $dir
            i32.const 3 i32.const 8 call $text local.set $file
            local.get $dir call $mkdir
            local.get $dir call $is_dir i32.const 1 i32.ne if unreachable end
            local.get $dir call $is_dir_new i32.const 1 i32.ne if unreachable end
            local.get $dir call $ls call $len if unreachable end
            call $stdin local.set $bytes
            local.get $file local.get $bytes call $write
            i32.const 11 i32.const 4 call $text local.get $bytes call $write_new if unreachable end
            local.get $file call $is_file i32.const 1 i32.ne if unreachable end
            local.get $dir call $ls_new if unreachable end
            call $entries call $len i32.const 1 i32.ne if unreachable end
            call $entries i32.const 0 call $get call $reader call $read
            i32.const 102 i32.ne if unreachable end
            call $cwd call $reader call $read i32.const -1 i32.eq if unreachable end
            local.get $file call $unlink local.get $dir call $rmdir
            local.get $dir call $exists if unreachable end
            local.get $dir call $mkdir_new if unreachable end
            local.get $dir call $rmdir_new if unreachable end))"#)).unwrap()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("input"), [0, 255, 42]).unwrap();
    let options = RunOptions::default()
        .with_working_directory(moonrun::WorkingDirectory::Fixed(
            directory.path().to_owned(),
        ))
        .with_stdio(
            std::fs::File::open(directory.path().join("input")).unwrap(),
            std::fs::File::create(directory.path().join("out")).unwrap(),
            std::fs::File::create(directory.path().join("err")).unwrap(),
        );
    assert_eq!(engine.run(&module, options).unwrap(), RunOutcome::Completed);
    assert_eq!(
        std::fs::read(directory.path().join("copy")).unwrap(),
        [0, 255, 42]
    );
    assert!(!directory.path().join("dir").exists());
}

#[test]
fn legacy_process_exit_stops_guest_execution() {
    let engine = Engine::default();
    let module = engine
        .compile(
            "exit.wasm",
            wat::parse_str(format!(
                r#"(module
        (import "__moonbit_sys_unstable" "is_windows" (func $windows (result i32)))
        (import "__moonbit_sys_unstable" "exit" (func $exit (param i32)))
        (func (export "_start")
            call $windows i32.const {} i32.ne if unreachable end
            i32.const 17 call $exit unreachable))"#,
                i32::from(cfg!(windows))
            ))
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        engine.run(&module, RunOptions::default()).unwrap(),
        RunOutcome::Exited(17)
    );
}

#[test]
fn compiler_produced_time_and_exit_use_legacy_imports() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = moon_test_util::test_dir::TestDir::from_case_root(
        manifest.join("tests/test_cases"),
        "test_engine_backend.in",
        false,
    );
    let moon = escargot::CargoBuild::new()
        .manifest_path(manifest.join("../moon/Cargo.toml"))
        .bin("moon")
        .current_release()
        .run()
        .expect("build moon");
    for target in ["wasm", "wasm-gc"] {
        for (profile, flag) in [("debug", "--debug"), ("release", "--release")] {
            snapbox::cmd::Command::new(moon.path())
                .current_dir(&directory)
                .args(["build", "--target", target, flag])
                .assert()
                .success();
            snapbox::cmd::Command::new(env!("CARGO_BIN_EXE_moonrun"))
                .arg(directory.join(format!("_build/{target}/{profile}/build/time/time.wasm")))
                .assert()
                .success()
                .stdout_eq("true\ntrue\n")
                .stderr_eq("");
            snapbox::cmd::Command::new(env!("CARGO_BIN_EXE_moonrun"))
                .arg(directory.join(format!("_build/{target}/{profile}/build/exit/exit.wasm")))
                .assert()
                .code(7)
                .stdout_eq("")
                .stderr_eq("");
        }
    }
}

#[test]
fn guest_gc_references_are_not_host_strings() {
    let engine = Engine::default();
    let wasm = wat::parse_str(r#"(module
        (type $record (struct (field i32)))
        (import "__moonbit_fs_unstable" "jsvalue_is_string" (func $is_string (param externref) (result i32)))
        (func (export "_start")
            i32.const 42 struct.new $record extern.convert_any
            call $is_string if unreachable end))"#).unwrap();
    let module = engine.compile("guest-reference.wasm", wasm);
    assert_eq!(
        engine.run(&module.unwrap(), RunOptions::default()).unwrap(),
        RunOutcome::Completed
    );
}
