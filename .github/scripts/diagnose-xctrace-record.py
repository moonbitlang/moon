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

"""Temporary standalone reproduction: xctrace records a short native workload."""

import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--repetitions", type=int, default=20)
args = parser.parse_args()
root = Path(os.environ["RUNNER_TEMP"]) / "xctrace-diagnostics"
root.mkdir(parents=True)
source = root / "workload.c"
source.write_text("""
#include <stdint.h>
#include <stdio.h>
#include <unistd.h>
int main(void) {
    volatile uint32_t checksum = 1;
    for (int batch = 0; batch < 20; batch++) {
        for (int i = 0; i < 10000000; i++)
            checksum = checksum * 1664525u + 1013904223u;
        usleep(50000);
    }
    printf("workload completed: %u\\n", checksum);
    return 0;
}
""")
binary = root / "workload"
subprocess.run(["clang", "-g", "-O2", str(source), "-o", str(binary)], check=True)

for trial in range(args.repetitions):
    case = root / str(trial)
    case.mkdir()
    command = ["xcrun", "xctrace", "record", "--quiet", "--template", "Time Profiler",
               "--no-prompt", "--output", str(case / "profile.trace"),
               "--target-stdout", str(case / "stdout.txt"), "--launch", "--", str(binary)]
    started = time.monotonic()
    sampled = set()
    timed_out = False
    with (case / "xctrace.log").open("w") as log, (case / "processes.jsonl").open("w") as events:
        child = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        while child.poll() is None:
            elapsed = time.monotonic() - started
            table = subprocess.check_output(["ps", "-axo", "pid=,ppid=,stat=,etime=,command="], text=True)
            # Instruments can launch the workload through a service, outside
            # xctrace's descendant tree. Match its unique executable path too.
            rows = [line for line in table.splitlines() if str(binary) in line or line.split()[0] == str(child.pid)]
            events.write(json.dumps({"seconds": elapsed, "processes": rows}) + "\n")
            events.flush()
            for threshold in (35, 70, 100):
                if elapsed >= threshold and threshold not in sampled:
                    sampled.add(threshold)
                    try:
                        subprocess.run(["sample", str(child.pid), "2", "-file", str(case / f"sample-{threshold}.txt")],
                                       stdout=log, stderr=subprocess.STDOUT, timeout=15)
                    except subprocess.TimeoutExpired:
                        log.write("sample exceeded 15 seconds\n")
            if elapsed >= 120:
                timed_out = True
                os.killpg(child.pid, signal.SIGKILL)
                break
            time.sleep(1)
        status = child.wait()
    result = {"trial": trial, "seconds": round(time.monotonic() - started, 2), "status": status, "timeout": timed_out}
    (case / "result.json").write_text(json.dumps(result))
    print(json.dumps(result), flush=True)
    if status:
        raise SystemExit(1)
