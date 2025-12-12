# Debugging MoonBuild

Moon ships several maintainer-facing utilities for debugging builds.

## Dumping dependency graphs

Four perma-unstable feature flags help inspect dependency graphs and rebuild decisions:

- `rr_export_module_graph` exports the module dependency graph in `target/`.
- `rr_export_package_graph` exports the package dependency graph in `target/`.
- `rr_export_build_plan` exports the build plan graph in `target/`.
- `rr_n2_explain` enables logging to explain why a build node is rebuilt.

The graph exporters emit [Graphviz][] DOT text.
Convert them locally with `dot -Tpng -o <output.png> <file>` (or any other Graphviz renderer),
or paste the output into an [online runner][graphviz-online].

> Note: Graphs can be HUGE! Try limiting the scope of the build graph before exporting.

Enable these flags via `moon -Z <flag1>,<flag2>,...`
or by setting `MOON_UNSTABLE=<flag1>,<flag2>,...`.

[graphviz]: https://graphviz.org/
[graphviz-online]: https://dreampuf.github.io/GraphvizOnline/

## Dumping build commands

`--dry-run` prints every build command
that would be executed under the current subcommand/flag set.

`--verbose` prints the command lines as they are executed,
and and also prints the test commands launched by `moon test`.

## Logging and tracing

Moon uses standard Rust logging conventions. Control verbosity with `RUST_LOG`:

- Specify a log level to enable more or less verbose logging: `RUST_LOG=info` (or: `debug`, `trace`, etc.)
- See more details about selective logging filters in [`tracing-subscriber`'s documentation][tracing]

For timing/task/thread details, set `MOON_TRACE=<level>`
(same syntax as `RUST_LOG`) or pass `--trace` (equivalent to `MOON_TRACE=trace`).
Moon writes a Chromium trace JSON file to the working directory;
open it in <https://ui.perfetto.dev> to inspect the timeline.

[tracing]: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html
