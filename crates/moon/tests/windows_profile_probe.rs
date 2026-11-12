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

//! Throwaway feasibility probe: capture a real native MoonBit process and retain
//! the system WPR trace, Rust-decoded samples, and PDB symbol results.

use std::path::{Path, PathBuf};
use std::time::Duration;

use snapbox::cmd::Command;

fn record_command(
    command: Command,
    artifacts: &Path,
    name: &str,
) -> std::io::Result<std::process::Output> {
    let result = command.timeout(Duration::from_secs(60)).output();
    let log = match &result {
        Ok(output) => format!(
            "status: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ),
        Err(error) => error.to_string(),
    };
    println!("{name}:\n{log}");
    std::fs::write(artifacts.join(format!("{name}.txt")), log)?;
    result
}

#[test]
#[cfg_attr(
    not(windows),
    ignore = "requires Windows WPR and the Samply ETL importer"
)]
fn probe_windows_cpu_profiler() {
    let temporary = tempfile::tempdir().unwrap();
    let artifacts = std::env::var_os("MOON_PROFILE_PROBE_ARTIFACT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().join("artifacts"));
    std::fs::create_dir_all(&artifacts).unwrap();

    // Explicitly use the inbox executable; an ADK install may also add WPR to PATH.
    let profiler = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/wpr.exe");
    let importer = PathBuf::from(std::env::var_os("MOON_PROFILE_PROBE_SAMPLY").unwrap());
    for (name, args) in [
        ("wpr-help", vec!["-help"]),
        ("wpr-profiles", vec!["-profiles"]),
        ("wpr-cpu-profile", vec!["-profiledetails", "CPU"]),
    ] {
        record_command(Command::new(&profiler).args(args), &artifacts, name).unwrap();
    }
    record_command(
        Command::new(&importer).arg("--version"),
        &artifacts,
        "samply-version",
    )
    .unwrap();

    let project = temporary.path().join("workload");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("moon.mod"), "name = \"test/profile_probe\"\n").unwrap();
    // /DEBUG makes the linker emit a PDB; --no-strip retains compiler symbols.
    std::fs::write(
        project.join("moon.pkg"),
        r#"import { "moonbitlang/core/async" }
pkgtype(kind: "executable")
options(link: { "native": { "cc-link-flags": "/DEBUG" } })
"#,
    )
    .unwrap();
    std::fs::write(
        project.join("main.mbt"),
        r#"async fn workload() -> Unit {
  let result = for batch = 0, checksum = 1; batch < 40; {
    let next = for i = 0, value = checksum; i < 10_000_000; {
      continue i + 1, (value * 33) ^ (value >> 5) ^ i
    } nobreak { value }
    @async.sleep(50)
    continue batch + 1, next
  } nobreak { checksum }
  println(result)
}

async fn main { workload() }
"#,
    )
    .unwrap();
    let build = record_command(
        Command::new(snapbox::cargo_bin!("moon"))
            .current_dir(&project)
            .env("MOON_DEP_CACHE", "off")
            .env("MOON_WORK", "off")
            .args(["build", "--target", "native", "--release", "--no-strip"]),
        &artifacts,
        "build",
    )
    .unwrap();
    assert!(build.status.success());
    let files: Vec<_> = walkdir::WalkDir::new(project.join("_build"))
        .into_iter()
        .map(|entry| entry.unwrap().into_path())
        .collect();
    let executable = files
        .iter()
        .find(|path| path.extension().is_some_and(|ext| ext == "exe"))
        .expect("native executable");
    assert!(
        files
            .iter()
            .any(|path| path.extension().is_some_and(|ext| ext == "pdb"))
    );
    for path in &files {
        if path
            .extension()
            .is_some_and(|ext| ext == "exe" || ext == "pdb")
        {
            std::fs::copy(path, artifacts.join(path.file_name().unwrap())).unwrap();
        }
    }

    let session = artifacts.join("workload.etl");
    // An explicit instance keeps this probe separate from the default WPR/UI
    // session. If it is already in use, fail instead of canceling another trace.
    let instance = "MoonProfileProbe";
    let start = record_command(
        Command::new(&profiler).args(["-start", "CPU", "-instancename", instance]),
        &artifacts,
        "start",
    )
    .unwrap();
    assert!(start.status.success());
    let workload = record_command(Command::new(executable), &artifacts, "workload");
    // Save and stop our session before inspecting workload failure or timeout.
    let stop = record_command(
        Command::new(&profiler)
            .arg("-stop")
            .arg(&session)
            .args(["-instancename", instance]),
        &artifacts,
        "stop",
    );
    // A failed stop can leave an ETW session alive. Never cancel the default
    // instance or a trace whose start did not succeed.
    if !stop.as_ref().is_ok_and(|output| output.status.success()) {
        let _ = record_command(
            Command::new(&profiler).args(["-cancel", "-instancename", instance]),
            &artifacts,
            "cancel-after-stop-failure",
        );
    }
    assert!(stop.unwrap().status.success());
    assert!(workload.unwrap().status.success());
    assert!(session.metadata().unwrap().len() > 0);

    // Samply's published Windows binary decodes ETL in Rust. Use only its import
    // path: its recording command depends on xperf, which is not part of Windows.
    let profile = artifacts.join("profile.json");
    let decoded = record_command(
        Command::new(&importer)
            .arg("import")
            .arg(&session)
            .args(["--save-only", "--name", "profile_probe", "-o"])
            .arg(&profile),
        &artifacts,
        "decode",
    )
    .unwrap();
    assert!(decoded.status.success());

    // The released importer produces module-relative addresses. Resolve only
    // addresses actually reachable from CPU samples against this build's PDB;
    // finding a workload name in an unused symbol table would prove nothing.
    let profile: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&profile).unwrap()).unwrap();
    let libraries = profile["libs"].as_array().unwrap();
    let pdb_file = artifacts
        .join(executable.file_name().unwrap())
        .with_extension("pdb");
    let pdb = pdb_addr2line::pdb::PDB::open(std::fs::File::open(pdb_file).unwrap()).unwrap();
    let data = pdb_addr2line::ContextPdbData::try_from_pdb(pdb).unwrap();
    let context = data.make_context().unwrap();
    let mut samples = Vec::new();
    let mut addresses = std::collections::BTreeMap::new();
    let mut workload_samples = 0;
    for thread in profile["threads"].as_array().unwrap() {
        for stack in thread["samples"]["stack"].as_array().unwrap() {
            let mut index = stack.as_u64();
            let mut frames = Vec::new();
            while let Some(stack_index) = index {
                let stack_index = stack_index as usize;
                let frame = thread["stackTable"]["frame"][stack_index].as_u64().unwrap() as usize;
                let function = thread["frameTable"]["func"][frame].as_u64().unwrap() as usize;
                let resource = thread["funcTable"]["resource"][function].as_u64();
                let library = resource.and_then(|resource| {
                    thread["resourceTable"]["lib"][resource as usize].as_u64()
                });
                let address = thread["frameTable"]["address"][frame].as_u64();
                if let (Some(library), Some(address)) = (library, address) {
                    let library = &libraries[library as usize];
                    if library["name"]
                        .as_str()
                        .unwrap()
                        .eq_ignore_ascii_case(executable.file_name().unwrap().to_str().unwrap())
                    {
                        let rva = u32::try_from(address).unwrap();
                        let names = addresses.entry(rva).or_insert_with(|| {
                            context
                                .find_frames(rva)
                                .unwrap()
                                .map(|frames| {
                                    frames
                                        .frames
                                        .into_iter()
                                        .filter_map(|frame| frame.function)
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default()
                        });
                        frames.extend(names.iter().cloned());
                    }
                }
                index = thread["stackTable"]["prefix"][stack_index].as_u64();
            }
            if frames.iter().any(|name| name.contains("workload")) {
                workload_samples += 1;
            }
            samples.push(serde_json::json!({
                "pid": thread["pid"], "tid": thread["tid"], "targetFrames": frames,
            }));
        }
    }
    let unresolved = addresses.values().filter(|names| names.is_empty()).count();
    let summary = serde_json::json!({
        "samples": samples.len(),
        "workloadSamples": workload_samples,
        "targetAddresses": addresses.len(),
        "unresolvedTargetAddresses": unresolved,
    });
    println!("Rust ETL/PDB result: {summary}");
    std::fs::write(
        artifacts.join("samples.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "summary": summary, "samples": samples, "addresses": addresses,
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(
        workload_samples > 0,
        "expected real CPU samples with workload symbols"
    );
}
