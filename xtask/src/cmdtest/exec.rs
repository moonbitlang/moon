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

use std::{
    ffi::OsStr,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct ExecResult {
    stdout: String,
    stderr: String,
    exit_code: u8,
}

fn moon_bin() -> PathBuf {
    snapbox::cmd::cargo_bin("moon")
}

pub fn moon_home() -> PathBuf {
    if let Ok(moon_home) = std::env::var("MOON_HOME") {
        return PathBuf::from(moon_home);
    }

    let h = home::home_dir();
    if h.is_none() {
        eprintln!("Failed to get home directory");
        std::process::exit(1);
    }
    let hm = h.unwrap().join(".moon");
    if !hm.exists() {
        std::fs::create_dir_all(&hm).unwrap();
    }
    hm
}

fn replace_dir(s: &str, dir: &impl AsRef<std::path::Path>) -> String {
    let path_str1 = dunce::canonicalize(dir)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    // for something like "{...\"loc\":{\"path\":\"C:\\\\Users\\\\runneradmin\\\\AppData\\\\Local\\\\Temp\\\\.tmpP0u4VZ\\\\main\\\\main.mbt\"...\r\n" on windows
    // https://github.com/moonbitlang/moon/actions/runs/10092428950/job/27906057649#step:13:149
    let s = s.replace("\\\\", "\\");
    let s = s.replace(&path_str1, "${WORK_DIR}");
    let s = s.replace(
        dunce::canonicalize(moon_home()).unwrap().to_str().unwrap(),
        "$MOON_HOME",
    );
    let s = s.replace(moon_bin().to_string_lossy().as_ref(), "moon");
    s.replace("\r\n", "\n").replace('\\', "/")
}

impl ExecResult {
    pub fn normalize(&self, workdir: &Path) -> String {
        let actual = if self.stderr.is_empty() {
            self.stdout.clone()
        } else {
            format!("{}\n{}", self.stdout, self.stderr)
        };
        let actual = if self.exit_code != 0 {
            format!("[{}]\n{}", self.exit_code, actual)
        } else {
            actual
        };

        replace_dir(&actual, &workdir)
    }
}

pub trait Executable<I, S, W>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
    W: AsRef<Path>,
{
    fn execute(&self, args: I, workdir: W) -> ExecResult;
}

#[derive(Debug)]
struct MoonExec;

impl<I, S, W> Executable<I, S, W> for MoonExec
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
    W: AsRef<Path>,
{
    fn execute(&self, args: I, workdir: W) -> ExecResult {
        let m = snapbox::cmd::cargo_bin("moon");
        let sys = SystemExec { cmd: m };
        sys.execute(args, workdir)
    }
}

#[derive(Debug)]
struct SystemExec {
    cmd: PathBuf,
}

impl<I, S, W> Executable<I, S, W> for SystemExec
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
    W: AsRef<Path>,
{
    fn execute(&self, args: I, workdir: W) -> ExecResult {
        let mut cmd = std::process::Command::new(&self.cmd);
        cmd.args(args)
            .current_dir(workdir.as_ref())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(err) => {
                return ExecResult {
                    stdout: String::new(),
                    stderr: err.to_string(),
                    exit_code: err.raw_os_error().map_or(255, |x| x as u8),
                }
            }
        };

        let mut stdout = String::new();
        let mut stderr = String::new();
        if let Some(ref mut out) = child.stdout {
            let _ = out.read_to_string(&mut stdout);
        }
        if let Some(ref mut err) = child.stderr {
            let _ = err.read_to_string(&mut stderr);
        }
        let exit_status = child.wait().unwrap();

        // a dirty workaround
        stderr = stderr
            .split('\n')
            .filter(|line| !line.starts_with("Blocking waiting for file lock"))
            .collect::<Vec<&str>>()
            .join("\n");

        ExecResult {
            stdout,
            stderr,
            exit_code: exit_status.code().map_or(255, |x| x as u8),
        }
    }
}

#[derive(Debug)]
struct CustomExec<F>
where
    F: Fn(&[String], &Path) -> String,
{
    func: F,
}

impl<F, I, S, W> Executable<I, S, W> for CustomExec<F>
where
    F: Fn(&[String], &Path) -> String,
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
    W: AsRef<Path>,
{
    fn execute(&self, args: I, workdir: W) -> ExecResult {
        let args_vec: Vec<String> = args
            .into_iter()
            .map(|s| s.as_ref().to_os_string().into_string().unwrap())
            .collect();
        let result = (self.func)(&args_vec, workdir.as_ref());
        ExecResult {
            stdout: result,
            stderr: String::new(),
            exit_code: 0,
        }
    }
}

mod custom_operations {
    use std::fs;
    use std::path::Path;

    pub fn xcat(args: &[String], workdir: &Path) -> String {
        if args.is_empty() {
            return "no file specified".to_string();
        }
        let file_path = workdir.join(&args[0]);
        match fs::read_to_string(file_path) {
            Ok(content) => content,
            Err(err) => format!("failed to read file: {}", err),
        }
    }

    pub fn xls(args: &[String], workdir: &Path) -> String {
        let dir = if args.is_empty() {
            workdir.to_path_buf()
        } else {
            workdir.join(&args[0])
        };
        match fs::read_dir(dir) {
            Ok(entries) => {
                let mut files = entries
                    .filter_map(Result::ok)
                    .map(|entry| {
                        entry
                            .path()
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .to_string()
                    })
                    .collect::<Vec<String>>();
                files.sort();
                files.join(" ")
            }

            Err(err) => format!("failed to list files: {}", err),
        }
    }
}

pub fn construct_executable<'a, I, S, W>(name: &str) -> Box<dyn Executable<I, S, W> + 'a>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr> + 'a,
    W: AsRef<Path> + 'a,
{
    match name {
        "moon" => Box::new(MoonExec),
        "xcat" => Box::new(CustomExec {
            func: custom_operations::xcat,
        }),
        "xls" => Box::new(CustomExec {
            func: custom_operations::xls,
        }),
        _ => Box::new(SystemExec {
            cmd: PathBuf::from(name),
        }),
    }
}
