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
    let toolchain = std::env::var_os("MOON_TOOLCHAIN_ROOT").unwrap();
    assert_eq!(
        args.next().unwrap(),
        std::fs::canonicalize(toolchain)
            .unwrap()
            .join("bin/mooncram.wasm")
    );
    assert_eq!(args.next().unwrap(), "--");
    assert_eq!(
        std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap(),
        std::fs::canonicalize(std::env::var_os("TESTCLI_EXPECTED_CWD").unwrap()).unwrap()
    );
    for arg in args {
        println!("arg={arg:?}");
    }
    std::io::copy(&mut std::io::stdin().lock(), &mut std::io::stdout().lock()).unwrap();
    eprintln!("runner stderr");
    std::process::exit(std::env::var("TESTCLI_EXIT_CODE").unwrap().parse().unwrap());
}
