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

fn main() {
    let mut args = std::env::args_os().skip(1);
    let toolchain = std::path::PathBuf::from(std::env::var_os("MOON_TOOLCHAIN_ROOT").unwrap());
    assert_eq!(
        std::fs::canonicalize(args.next().unwrap()).unwrap(),
        std::fs::canonicalize(toolchain.join("bin/moon_new.wasm")).unwrap()
    );
    assert_eq!(
        args.collect::<Vec<_>>(),
        ["--", "--user=moonbitlang", "--name=hello", "--", "-new project"]
    );
    assert_eq!(
        std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap(),
        std::fs::canonicalize(std::env::var_os("MOON_NEW_EXPECTED_CWD").unwrap()).unwrap()
    );
    println!("generator stdout");
    eprintln!("generator stderr");
    std::process::exit(std::env::var("MOON_NEW_EXIT_CODE").unwrap().parse().unwrap());
}
