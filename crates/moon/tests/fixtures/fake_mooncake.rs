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

use std::io::Read;

fn main() {
    assert_eq!(
        std::env::args().skip(1).collect::<Vec<_>>(),
        ["--read-args-from-stdin"]
    );
    assert!(std::path::PathBuf::from(std::env::var_os("MOON_OVERRIDE").unwrap()).is_file());
    let mut payload = String::new();
    std::io::stdin().read_to_string(&mut payload).unwrap();
    std::fs::write("handoff.json", payload).unwrap();
    println!("backend result");
    eprintln!("backend notice");
}
