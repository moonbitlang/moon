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
//! the profiler's help, configuration, and raw artifacts for format inspection.

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
#[cfg_attr(not(windows), ignore = "requires the Windows Visual Studio collector")]
fn probe_windows_cpu_profiler() {
    let temporary = tempfile::tempdir().unwrap();
    let artifacts = std::env::var_os("MOON_PROFILE_PROBE_ARTIFACT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().join("artifacts"));
    std::fs::create_dir_all(&artifacts).unwrap();

    let vswhere = PathBuf::from(std::env::var_os("ProgramFiles(x86)").unwrap())
        .join("Microsoft Visual Studio/Installer/vswhere.exe");
    let discovery = record_command(
        Command::new(vswhere).args(["-latest", "-products", "*", "-property", "installationPath"]),
        &artifacts,
        "vswhere",
    )
    .unwrap();
    assert!(discovery.status.success());
    let installation = String::from_utf8(discovery.stdout).unwrap();
    let collector = Path::new(installation.trim()).join("Team Tools/DiagnosticsHub/Collector");
    let profiler = collector.join("VSDiagnostics.exe");
    assert!(profiler.is_file(), "VSDiagnostics is missing");
    let inventory = walkdir::WalkDir::new(collector.parent().unwrap())
        .into_iter()
        .map(|entry| entry.unwrap().path().display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(artifacts.join("tool-inventory.txt"), inventory).unwrap();
    for (name, args) in [
        ("help", vec!["/?"]),
        ("start-help", vec!["start", "/?"]),
        ("stop-help", vec!["stop", "/?"]),
        ("export-help", vec!["export", "/?"]),
    ] {
        record_command(Command::new(&profiler).args(args), &artifacts, name).unwrap();
    }
    let config = collector.join("AgentConfigs/CPUUsageBase.json");
    std::fs::copy(&config, artifacts.join("CPUUsageBase.json")).unwrap();

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

    let session = artifacts.join("workload.diagsession");
    let start = record_command(
        Command::new(&profiler).args([
            "start".to_string(),
            "1".to_string(),
            format!("/launch:{}", executable.display()),
            format!("/loadConfig:{}", config.display()),
        ]),
        &artifacts,
        "start",
    );
    std::thread::sleep(Duration::from_secs(15));
    // Stop even if start failed or timed out, before asserting on its result.
    let stop = record_command(
        Command::new(&profiler).args([
            "stop".to_string(),
            "1".to_string(),
            format!("/output:{}", session.display()),
        ]),
        &artifacts,
        "stop",
    );
    assert!(start.unwrap().status.success());
    assert!(stop.unwrap().status.success());
    assert!(session.metadata().unwrap().len() > 0);

    // Inspect the actual container before choosing a parsing approach.
    match zip::ZipArchive::new(std::fs::File::open(&session).unwrap()) {
        Ok(mut archive) => {
            let entries = archive.file_names().collect::<Vec<_>>().join("\n");
            println!("diagsession entries:\n{entries}");
            std::fs::write(artifacts.join("session-entries.txt"), entries).unwrap();
            archive.extract(artifacts.join("session")).unwrap();
        }
        Err(error) => {
            println!("diagsession is not a readable ZIP: {error}");
            std::fs::write(artifacts.join("container-error.txt"), error.to_string()).unwrap();
        }
    }
}
