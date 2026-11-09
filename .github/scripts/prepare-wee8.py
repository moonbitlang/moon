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

"""Verify release downloads before using wee8's documented offline overrides."""

import gzip
import hashlib
import os
from pathlib import Path
import shutil

directory = Path(os.environ["RUNNER_TEMP"]) / "wee8"
downloads = directory / "downloads"
target = os.environ["TARGET"]
asset = os.environ["ASSET"]

# Producer manifests use either text or binary records, optionally prefixed "./".
checksums = {}
for line in (downloads / f"SHA256SUMS_{target}").read_text().splitlines():
    checksum, name = line.split(maxsplit=1)
    name = name.removeprefix("*").removeprefix("./")
    if name in checksums:
        raise ValueError(f"Duplicate checksum for {name}")
    checksums[name] = checksum

# Verify all inputs even on cache hits. The crate still checks its independently
# pinned header hashes before bindgen, enforcing the native ABI contract.
for name in (asset, f"wee8_c_api_{target}.h", f"wasm_c_api_{target}.h"):
    digest = hashlib.sha256()
    with (downloads / name).open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    if digest.hexdigest() != checksums[name].lower():
        raise ValueError(f"wee8 checksum mismatch for {name}")

archive = directory / ("wee8.lib" if target.endswith("-msvc") else "libwee8.a")
temporary = archive.with_suffix(".part")
with gzip.open(downloads / asset, "rb") as source, temporary.open("wb") as output:
    shutil.copyfileobj(source, output)
temporary.replace(archive)
shutil.copyfile(downloads / f"wee8_c_api_{target}.h", directory / "wee8.h")
shutil.copyfile(downloads / f"wasm_c_api_{target}.h", directory / "wasm.h")

with open(os.environ["GITHUB_ENV"], "a") as output:
    output.write(f"WEE8_ARCHIVE={archive}\nWEE8_HEADER={directory / 'wee8.h'}\n")
