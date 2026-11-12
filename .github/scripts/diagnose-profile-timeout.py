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
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""Temporary profiler timeout probe; preserves the tests and their deadlines."""

import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--repetitions", type=int, default=5)
parser.add_argument("--suite-first", action="store_true")
args = parser.parse_args()
args.output = args.output.resolve()
args.output.mkdir(parents=True, exist_ok=True)


def process_table():
    output = subprocess.check_output(
        ["ps", "-axo", "pid=,ppid=,stat=,etime=,command="], text=True
    )
    rows = {}
    for line in output.splitlines():
        fields = line.split(None, 4)
        if len(fields) == 5:
            rows[int(fields[0])] = {
                "pid": int(fields[0]),
                "parent": int(fields[1]),
                "state": fields[2],
                "elapsed": fields[3],
                "command": fields[4],
            }
    return rows


def capture_sample(pid, destination):
    with destination.with_suffix(".log").open("w") as log:
        try:
            subprocess.run(
                ["sample", str(pid), "2", "-file", str(destination)],
                stdout=log,
                stderr=subprocess.STDOUT,
                timeout=15,
            )
        except subprocess.TimeoutExpired:
            log.write("sample exceeded 15 seconds\n")


modes = (["suite"] if args.suite_first else []) + ["pair"] * args.repetitions
for index, mode in enumerate(modes):
    case = args.output / f"{index:02}-{mode}"
    case.mkdir()
    fixtures = case / "fixtures"
    env = os.environ.copy()
    env["MOON_TEST_KEEP_DIR"] = str(fixtures)
    command = ["cargo", "test", "-p", "moon", "--test", "mod"]
    if mode == "pair":
        command.append("profile_with_real_profiler")
    command += ["--", "--nocapture"]
    print(json.dumps({"trial": index, "mode": mode, "command": command}), flush=True)
    started = time.monotonic()
    known = set()
    profilers = {}
    sampled = set()
    with (case / "tests.log").open("w") as log, (case / "processes.jsonl").open("w") as events:
        child = subprocess.Popen(
            command, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True
        )
        known.add(child.pid)
        while child.poll() is None:
            rows = process_table()
            while True:
                expanded = known | {pid for pid, row in rows.items() if row["parent"] in known}
                if expanded == known:
                    break
                known = expanded
            active = [row for pid, row in rows.items() if pid in known]
            events.write(json.dumps({"seconds": time.monotonic() - started, "processes": active}) + "\n")
            events.flush()
            for row in active:
                if "xctrace record " not in row["command"] and "xctrace export " not in row["command"]:
                    continue
                pid = row["pid"]
                if pid not in profilers:
                    profilers[pid] = (time.monotonic(), row["command"])
                    print(json.dumps({"trial": index, "profiler": row}), flush=True)
                age = time.monotonic() - profilers[pid][0]
                for threshold in (35, 70, 100):
                    if age >= threshold and (pid, threshold) not in sampled:
                        sampled.add((pid, threshold))
                        capture_sample(pid, case / f"sample-{pid}-{threshold}.txt")
            # This guard is outside the tests' unchanged 120-second command timeout.
            if time.monotonic() - started > (900 if mode == "suite" else 300):
                log.write("[DEBUG-profile-repro] outer watchdog expired\n")
                log.flush()
                os.killpg(child.pid, signal.SIGKILL)
                break
            time.sleep(2)
        status = child.wait()
    result = {"trial": index, "mode": mode, "seconds": round(time.monotonic() - started, 2), "status": status}
    (case / "result.json").write_text(json.dumps(result, indent=2))
    print(json.dumps(result), flush=True)
    if status:
        rows = process_table()
        for pid, (_, command) in profilers.items():
            if pid in rows and rows[pid]["command"] == command:
                capture_sample(pid, case / f"sample-{pid}-after-failure.txt")
                # Kill only a profiler observed under this test invocation.
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
        for index, trace in enumerate(fixtures.rglob("profile.trace")):
            shutil.copytree(trace.parent, case / "profiles" / str(index))
    shutil.rmtree(fixtures, ignore_errors=True)
    if status:
        print((case / "tests.log").read_text()[-24000:], flush=True)
        raise SystemExit(1)
