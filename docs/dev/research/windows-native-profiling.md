# Windows native CPU profiling

Researched on 2026-11-12 against official documentation and first-party source.
This is non-normative research for profiling MoonBit native executables. It
does not define implemented `moon` behavior or establish a production dependency.

## Finding

An inbox WPR collector followed by a Rust ETL consumer and PDB symbolication is
a credible prototype path without a .NET dependency. Windows includes the WPR
command-line tool; the ADK supplies WPR's GUI, WPA, and `xperf`. Samply provides
an existing Rust implementation to reuse: its Windows `record` collector calls
`xperf`, but its separate ETL `import` path reads an existing file directly.
The proposed WPR substitution is an inference from these separate collection
and consumption interfaces, not a claim that every WPR profile has already
been validated with MoonBit. [Microsoft tool availability][wpr-tools],
[samply collector][samply-xperf], [released ETL importer][samply-import]

Keep three responsibilities distinct:

| Responsibility | Candidate | What it must establish |
| --- | --- | --- |
| Collect CPU samples and stack addresses | Inbox `wpr` with a CPU recording profile | Start before the target; finish and save the owned ETW session |
| Interpret ETL events | Released Windows `samply import` CLI | Sample/stack correlation, process and thread lifetimes, image mappings, lost events |
| Resolve addresses | `pdb-addr2line`, or the higher-level `wholesym` | Correct executable/PDB identity, relative addresses, functions, source lines and inlines |

The Windows `ProcessTrace` API delivers ETW events to consumers; it is not a
managed-runtime API. Samply's reader uses `OpenTraceW` and `ProcessTrace`
through the Rust `windows` bindings. PDB decoding is a separate Rust library
operation. [ProcessTrace][process-trace], [samply reader][samply-reader],
[wholesym][wholesym]

## WPR deployment and session ownership

WPR accepts built-in profiles such as `CPU` and custom `.wprp` files.
`-filemode` preserves a recording in an unbounded file; the default is memory
mode. `-stop` saves an ETL file, whereas `-cancel` discards the recording.
`-skipPdbGen` skips generation of .NET native-image/embedded PDBs; it does not
create debug information for MoonBit native binaries. [WPR command reference][wpr-cli]

Use an explicit instance name on **every** lifecycle operation. Without one,
WPR selects its default `WprApp` instance. Starting with a name and stopping
without that name does not stop the named trace. The Microsoft WPR team's
examples explicitly support independently starting, querying, cancelling and
stopping named recordings. A collection sequence to validate is:
[WPR session naming][wpr-instance]

```text
wpr -start CPU -filemode -instancename MoonNativeProfile
wpr -status -instancename MoonNativeProfile
wpr -stop profile.etl -skipPdbGen -instancename MoonNativeProfile
```

The target executes between start and stop. Cancellation, when the recording
is being abandoned, must also specify `-instancename MoonNativeProfile`.
This example is a single-owner sequence, not a concurrency policy.

ETW sessions outlive the process that starts them. Microsoft recommends
descriptive, stable owner names, explicit cleanup of orphaned sessions, and
minimal session lifetime. Names do not eliminate shared resource limits:
Windows permits at most eight system loggers. Therefore, Moon should serialize
ownership or report a conflict; it must not blindly cancel a default session
or a session belonging to another live invocation. This ownership policy is a
design inference from the API contract. [StartTrace lifecycle and limits][start-trace]

Plan for elevated collection. The generic ETW API allows some sessions to be
controlled by Performance Log Users, but explicitly restricts the NT Kernel
Logger to administrators or LocalSystem. BenchmarkDotNet also requires admin
for its kernel CPU collection. These are not evidence that arbitrary
unelevated WPR CPU capture works. Parsing a saved ETL and reading symbols are
separate operations. [StartTrace permissions][start-trace],
[BenchmarkDotNet EtwProfiler][bdn]

Inbox and ADK builds need not expose identical features. The WPR team documents
a custom-profile parsing bug in build 17763 fixed in later builds. Prefer
probing the selected WPR's profiles/options and validating the actual CPU
events over assuming a newer ADK reference implies inbox support for every
custom profile. [WPR team's compatibility discussion][wpr-instance]

## What samply already implements

Samply's collector enables process/thread, image-load, CPU-profile and context
switch events, with stack walking for profile and context switches. Its Rust
consumer interprets the resulting ETL separately. This is a useful architectural
precedent for replacing only the collection frontend. [Collector source][samply-xperf],
[ETL conversion source][samply-converter]

The consumer illustrates correctness requirements beyond reading addresses:

- `StackWalk/Stack` carries its own `EventTimeStamp`; the event header timestamp
  is the emission time and is not the sampled-stack timestamp.
- `SampleProf` identifies a thread; thread-to-process mapping and PID/TID
  lifetimes matter, including identifier reuse.
- Image load/rundown events establish address ranges. Samply also consumes
  image/PDB metadata synthesized during `xperf` merging; unmerged files do not
  have that metadata.
- Trace headers report lost events, and the converter reports them.

These facts come from the [ETL converter][samply-converter] and
[profile context][samply-context]. A WPR implementation must verify its saved
ETL contents rather than assume `xperf`'s merge behavior. A retained target
executable and matching PDB provide another source of image/debug identity.
Samply's symbol reader checks that the PDB debug ID matches the PE's expected
ID. [PE/PDB identity validation][samply-pdb]

Reuse options and limits:

- **`ferrisetw`** exposes `FileTrace::new` for ETL files and a blocking event
  consumer. However, version 1.2.0's file builder exposes no raw-timestamp
  option. Samply explicitly selects `PROCESS_TRACE_MODE_RAW_TIMESTAMP`, which
  keeps sample timestamps comparable with stack payload timestamps. Adopting
  `ferrisetw` would therefore require additional timestamp handling or a reader
  change, as well as kernel-schema validation; the proposed prototype does not
  establish it as a drop-in replacement. [FileTrace API][ferrisetw],
  [builder source][ferrisetw-source], [samply reader][samply-reader]
- **Samply's `etw_reader`** is currently an internal copy, explicitly described
  as newer than the published `etw-reader` crate. It registers custom schemas
  for sampled-profile, thread and image/PDB events. Treat it as implementation
  evidence, not a ready-made stable dependency with identical published
  behavior. [Reader note][samply-reader-note], [custom schemas][samply-schemas]
- **`wholesym`** resolves local PDBs, Windows symbol servers, DWARF-in-PE and
  fallback symbols. It uses `pdb-addr2line` for PDB decoding and returns source
  and inline-frame information. `/DEBUG:FASTLINK` PDBs are unsupported; a full
  matching PDB is the appropriate initial test input. A direct
  `pdb-addr2line` dependency is a smaller candidate when symbol discovery and
  downloading are unnecessary. [wholesym capabilities][wholesym],
  [FASTLINK issue][fastlink]

For the first experiment, reuse the released Windows samply 0.13.1 executable
as a converter instead of copying its internal ETW modules:

```text
samply import workload.etl --save-only --name profile_probe -o profile.json
```

The release's CLI dispatches `.etl` input directly to the Windows importer,
and `--save-only` suppresses the local server. Here `--name profile_probe`
filters process names by substring; use the actual workload name, or `--pid`
with its captured process ID. The importer converts the events
and writes the profile without invoking `xperf`; this is distinct from
`samply record`. Its ETL conversion function does not perform a presymbolication
pass, so the experiment should resolve sampled target-module relative
addresses explicitly with `pdb-addr2line`, rather than rely on
`--unstable-presymbolicate`. This reuses the existing event correlation while
keeping the experiment independent of .NET and a private ETW-reader fork.
[Released CLI dispatch][samply-main], [released ETL importer][samply-import]

This proves a CLI-based composition if successful. Embedding an ETW library
inside Moon remains a separate dependency/API decision; a working external
converter does not make samply's internal reader a supported library API.

## Recorded experiment

The [Windows CI probe][probe-job] passed using inbox WPR
10.0.26100.33648 on Windows Server 2025, Samply 0.13.1 for ETL import, and
`pdb-addr2line` 0.12.1 for symbols. It built and ran a native MoonBit executable
alternating checksum work with async sleep. The collection/decoding path did
not invoke `xperf`, VSDiagnostics, or a managed decoder. The compiler emitted
a full PDB with `/DEBUG`.

The [retained artifacts][probe-artifacts] establish:

- WPR started, the workload exited successfully, and WPR stopped and saved
  its ETL. The test finished in 30.07 seconds.
- The imported profile contains 793 sample-table rows, of which 674 have
  target frames containing the MoonBit workload function.
- All 37 distinct target-module addresses reachable from these rows resolve
  against the build's PDB. This checks addresses actually used in stacks,
  rather than merely finding the function in a symbol file.

These are **decoded profile rows, not a raw ETW CPU-event count**. The default
WPR CPU profile also enables context-switch events, and Samply represents
off-CPU intervals in its sample table. In this recording, 41 rows have zero
`threadCPUDelta`; all 674 workload rows have a positive CPU delta. Samply's
conversion source explains the extra rows and their weights.
[Sample conversion][samply-release-context]

The profile's `meta.interval` is approximately 0.1221 ms, the released
importer's initial value, while consecutive workload-region samples are
typically about 1 ms apart. This run therefore validates collection and
function-stack resolution, not a ready-to-use CPU-time report. Production
integration must interpret CPU versus waiting records and obtain the actual
sampling interval instead of trusting that default. The released importer
initializes that value before processing ETL; its collection-start handler
can replace it when the corresponding event is recognized.
[Importer initialization][samply-import],
[Collection-start handling][samply-release-context]

This experiment covers one Windows/architecture/toolchain combination and
normal completion. It does not establish interruption behavior, trace-loss
accounting, arbitrary symbol discovery, or interoperability across all inbox
WPR versions.

## Other language and framework precedents

### Go: runtime-owned sampling

Go's Windows runtime waits on a profiling timer, walks its runtime thread
list, skips known blocked threads, suspends a thread, obtains its context,
invokes its own stack profiler, then resumes it. This approach knows Go's
goroutine and scheduler structures; it is not an external profiler that can
simply be applied to arbitrary native executables.
[Go Windows runtime, `profileLoop` and `profilem`][go-runtime]

The maintainers' discussion in Go issue #61665 is a concrete example of Windows
profiling pitfalls: increased requested frequency caused fewer effective
samples because of timer resolution. The fix used high-resolution timers.
The current source also tests whether a high-resolution timer accepts a
periodic interval and falls back when necessary. This is evidence that a
home-grown suspend/sample loop adds timer and scheduler correctness work.
[Go issue #61665][go-issue], [current timer implementation][go-runtime]

### Python / py-spy: external but runtime-aware sampling

Py-spy reads CPython process memory through `ReadProcessMemory` on Windows and
walks interpreter/thread/frame structures. It supports optional native frames;
the native path uses `remoteprocess`, whose Windows implementation suspends
threads and unwinds them with `GetThreadContext` and `StackWalk64`.
[py-spy design][py-spy], [native integration][py-spy-native],
[thread suspension][remoteprocess], [Windows unwinder][remoteprocess-unwind]

Py-spy documents limitations in identifying idle Windows threads, including
I/O waits. Consequently, polling stacks with an idle heuristic should not be
assumed to have the same CPU-time meaning as kernel CPU samples. For Moon,
this is a possible alternative architecture requiring validation, rather than
an ETL parser replacement. [py-spy idle detection][py-spy]

### BenchmarkDotNet / .NET: ETW plus runtime events

BenchmarkDotNet starts kernel and user ETW sessions before launching the
benchmark. The kernel session captures CPU stacks and hardware counters; the
user session captures GC/JIT events. It stops both sessions and merges their
files afterwards. `TraceEvent` is its managed implementation choice, while ETW
is the underlying collection mechanism. Its documented defaults include
image-load and profile events. [EtwProfiler design][bdn]

EventPipe solves a different problem: it is a .NET runtime component for
managed/runtime events. Microsoft explicitly directs native CPU sampling and
kernel-event users to OS tracing such as ETW. Therefore EventPipe does not
provide a dependency-free profiler for a MoonBit native executable.
[Microsoft EventPipe comparison][eventpipe]

## Prototype acceptance boundary

The preceding experiment covers the basic collection and symbol-resolution
path. A production implementation still needs the following checks:

1. Capture a native workload with inbox WPR and consume its ETL using Rust,
   with no `xperf`, Visual Studio diagnostics tool, or managed helper involved.
2. Attribute CPU samples to the launched process and resolve known nested
   MoonBit functions against the matching PDB. Keep missing symbols visible;
   do not turn an undecoded trace into an empty successful profile.
3. Distinguish CPU samples from waits/context switches, and report dropped
   events or missing sample/stack correlations.
4. Exercise normal completion, target failure, interruption, start/stop
   failure, and an already-running named session; preserve unrelated traces.
5. Validate PE/PDB identity and image address conversion before expanding to
   other architectures, GNU-like debug formats, or runtime/JIT integrations.

This keeps the first investigation focused on native CPU sampling. Runtime
events, symbol-server policy, and a general cross-platform profiler remain
separate decisions.

[wpr-tools]: https://learn.microsoft.com/en-us/troubleshoot/windows-server/support-tools/support-tools-xperf-wpa-wpr
[wpr-cli]: https://learn.microsoft.com/en-us/windows-hardware/test/wpt/wpr-command-line-options
[wpr-instance]: https://devblogs.microsoft.com/performance-diagnostics/controlling-the-event-session-name-with-the-instance-name/
[start-trace]: https://learn.microsoft.com/en-us/windows/win32/api/evntrace/nf-evntrace-starttracew
[process-trace]: https://learn.microsoft.com/en-us/windows/win32/api/evntrace/nf-evntrace-processtrace
[samply-xperf]: https://github.com/mstange/samply/blob/main/samply/src/windows/xperf.rs
[samply-reader]: https://github.com/mstange/samply/blob/main/samply/src/windows/etw_reader/mod.rs
[samply-reader-note]: https://github.com/mstange/samply/blob/main/samply/src/windows/etw_reader/README.md
[samply-schemas]: https://github.com/mstange/samply/blob/main/samply/src/windows/etw_reader/custom_schemas.rs
[samply-converter]: https://github.com/mstange/samply/blob/main/samply/src/windows/etw_gecko.rs
[samply-context]: https://github.com/mstange/samply/blob/main/samply/src/windows/profile_context.rs
[samply-pdb]: https://github.com/mstange/samply/blob/main/samply-symbols/src/windows.rs
[wholesym]: https://github.com/mstange/samply/blob/main/wholesym/README.md
[fastlink]: https://github.com/mstange/pdb-addr2line/issues/53
[ferrisetw]: https://docs.rs/ferrisetw/1.2.0/ferrisetw/trace/struct.FileTrace.html
[ferrisetw-source]: https://docs.rs/ferrisetw/1.2.0/src/ferrisetw/trace.rs.html#555-610
[samply-main]: https://github.com/mstange/samply/blob/samply-v0.13.1/samply/src/main.rs
[samply-import]: https://github.com/mstange/samply/blob/samply-v0.13.1/samply/src/windows/import.rs
[go-runtime]: https://github.com/golang/go/blob/master/src/runtime/os_windows.go
[go-issue]: https://github.com/golang/go/issues/61665
[py-spy]: https://github.com/benfred/py-spy/blob/master/README.md
[py-spy-native]: https://github.com/benfred/py-spy/blob/master/src/native_stack_trace.rs
[remoteprocess]: https://github.com/benfred/remoteprocess/blob/master/src/windows/mod.rs
[remoteprocess-unwind]: https://github.com/benfred/remoteprocess/blob/master/src/windows/unwinder.rs
[bdn]: https://benchmarkdotnet.org/articles/features/etwprofiler.html
[eventpipe]: https://learn.microsoft.com/en-us/dotnet/core/diagnostics/eventpipe
[probe-job]: https://github.com/moonbitlang/moon/actions/runs/43161483818/job/130993546496
[probe-artifacts]: https://github.com/moonbitlang/moon/actions/runs/43161483818/artifacts/13918013261
[samply-release-context]: https://github.com/mstange/samply/blob/samply-v0.13.1/samply/src/windows/profile_context.rs
