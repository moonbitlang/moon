# moon: The build system and package manager for MoonBit.
# Copyright (C) 2026 International Digital Economy Academy
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
# either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
#
# For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

"""Exercise the packaged Moon and Moonrun using existing compiler fixtures."""

import os
from pathlib import Path
import shutil
import subprocess
import sys

binaries = Path(sys.argv[1]).resolve()
project = Path(sys.argv[2]).resolve()
root = Path(__file__).resolve().parents[2]
suffix = ".exe" if os.name == "nt" else ""
moon = binaries / f"moon{suffix}"
moonrun = binaries / f"moonrun{suffix}"
# An explicit override prevents a successful test from silently using the
# installed toolchain's Moonrun instead of the release binary under test.
env = {**os.environ, "MOONRUN_OVERRIDE": str(moonrun)}
shutil.copytree(
    root / "crates/moonrun/tests/test_cases/test_engine_backend.in",
    project,
    ignore=shutil.ignore_patterns("_build", ".mooncakes"),
)
subprocess.run([str(moon), "version", "--all"], env=env, check=True)
subprocess.run([str(moonrun), "--version"], env=env, check=True)
for target in ("wasm", "wasm-gc"):
    # Match the existing ffi-bytes integration case: only WasmGC has this ABI.
    env["MOONC_INTERNAL_PARAMS"] = "use_ffi_bytes=1|" if target == "wasm-gc" else ""
    subprocess.run(
        [str(moon), "-C", str(project), "build", "--target", target, "--release"],
        env=env,
        check=True,
    )
    for package, expected in (
        ("js_string", b"hello from imported constants\n11\nfalse\n"),
        ("ffi_bytes", b"A\n"),
    ):
        wasm = project / f"_build/{target}/release/build/{package}/{package}.wasm"
        result = subprocess.run(
            [str(moonrun), str(wasm)], env=env, capture_output=True, check=True
        )
        assert result.stdout.replace(b"\r\n", b"\n") == expected, result
        assert result.stderr == b"", result
    # Exercise Moon's subprocess selection and the compiler-generated test driver.
    result = subprocess.run(
        [str(moon), "-C", str(project), "test", "--target", target,
         "--release", "--no-parallelize"],
        env=env,
        capture_output=True,
        check=True,
    )
    assert result.stdout.replace(b"\r\n", b"\n") == (
        b"Total tests: 1, passed: 1, failed: 0.\n"
    ), result
    print(f"{target}: release strings, bytes and Moon test driver passed", flush=True)
