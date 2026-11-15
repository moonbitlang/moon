# `moon fix`

`moon fix` applies the machine-applicable fixes that moonc attaches to JSON
diagnostics. moonc only proposes fixes; `moon fix` is the fix driver that
decides which of them are safe to apply. The compiler documents the contract
in `lib/xml/computil/diagnostic_fix.mli` of its repository.

## One round

1. Run the same check as `moon check` for the selected paths and target
   backends, with JSON diagnostics, no diagnostic limit, no patch file, and
   without publishing `pkg.generated.mbti` (it would describe the code before
   its fixes). A failed check still proposes fixes: moonc reports some
   warnings, such as an unused `mut`, as errors by default.
2. From the execution plan, record which local source files each Target
   Backend's compiler actions read. A file of a backend may be read by several
   actions (the library check, the whitebox test check, a doctest-only check);
   their diagnostics are deduplicated, so a backend proposes the union of the
   fixes of its actions.
3. Take the first fix of each diagnostic as that backend's proposal.
4. Select fixes with the driver checks in
   [`cli/fix/driver.rs`](../../../crates/moon/src/cli/fix/driver.rs): a fix is
   applied only if every backend that compiled each file it edits proposes it,
   and every edit still reads its `expect` text, joins no tokens, and so on. A
   fix that fails a check is skipped as a whole. Fixes are selected greedily
   in file order; a fix whose edits overlap or start at the same point as
   those of a selected fix is skipped, and so is one whose edits touch them
   (as adjacent escapes in one literal do) when their combined result joins
   tokens.
5. Write each changed file, unless it changed while the round ran.

Rounds repeat, each with a new check, until a round applies nothing or four
rounds have run. A later round picks up fixes that conflicted with an applied
one, and fixes that the applied ones make possible.

## Output

The Command Result on stdout lists `path:line:column: title` for each applied
fix, with paths relative to the current directory. `--diff` instead prints a
unified diff of one round and writes nothing. A summary of applied and skipped
fixes is a status line on stderr; `--verbose` explains each skipped fix of the
last round. The exit status is 0 unless the command itself fails; run
`moon check` to see the remaining diagnostics. `--dry-run` prints the check
commands and applies nothing.
