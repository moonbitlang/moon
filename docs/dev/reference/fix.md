# `moon fix`

The experimental `moon fix` command applies the machine-applicable fixes that
moonc attaches to JSON diagnostics. moonc only proposes fixes; `moon fix` is the
fix driver that decides which of them are safe to apply. The compiler documents
the contract in `lib/xml/computil/diagnostic_fix.mli` of its repository.

## One round

1. Run the same check as `moon check` for the selected paths and target
   backends, with JSON diagnostics, no diagnostic limit, no patch file, and
   without publishing `pkg.generated.mbti` (it would describe the code before
   its fixes). Enable warning `+99` (`pending_deprecated`) to request migration
   fixes for pending deprecations. Explicit `--warn-list` settings follow `+99`,
   so users can override it with `--warn-list -99`. Ordinary compiler errors may
   still propose fixes, such as an unused `mut`. Tool failures,
   executor-reported interruption, and compiler
   failures without error diagnostics stop the run before applying any fixes
   from that check.
2. From the execution plan, record which editable local source files each
   Target Backend's compiler actions read. A file selector permits edits only
   to that file; a directory selector permits edits only to that package.
   Checking a dependency does not grant permission to edit it. With no selector,
   all local packages are eligible. Declared prebuild outputs are excluded,
   including outputs declared by another package, outputs already on disk,
   outputs whose generation is disabled, and symlink aliases of existing outputs.
   File symlinks are also excluded: compiling a link in one package does not
   authorize changing a target shared with other packages or outside the project.
   Explicit file-symlink selectors are skipped with a warning before package
   selection can follow them. If every explicit selector is skipped, the command
   stops without falling back to the whole project. Directory aliases remain
   supported. Fixes involving file symlinks are skipped as a whole, including in
   `--diff` previews; `--verbose` explains these skipped fixes.
   A file of a backend may be read by several actions (the library check, the
   whitebox test check, a doctest-only check);
   their diagnostics are deduplicated, so a backend proposes the union of the
   fixes of its actions.
3. Take the first fix of each diagnostic as that backend's proposal.
4. Select fixes with the driver checks in
   [`cli/fix/driver.rs`](../../../crates/moon/src/cli/fix/driver.rs): a fix is
   applied only if every backend that compiled each file it edits proposes it,
   and every edit still reads its `expect` text, joins no tokens, and so on. A
   fix that fails a check is skipped as a whole. Fixes are selected greedily
   in file order; a fix whose edits overlap or start at the same point as
   those of a selected fix is skipped. Selected fixes must also preserve each
   other's file-wide `fresh` constraints: neither may introduce a whole
   identifier reserved by the other, including through an edit with no `fresh`
   constraint of its own. Complete groups of touching edits are checked together
   for joined tokens, after all their replacements have been accumulated.
5. Stage the complete replacement in the source file's directory, preserving
   its permissions. Reject files that became symlinks after planning. Recheck
   that the source still matches the driver's original content, then replace it
   without resolving the final path component. Failed staging or
   replacement leaves that file intact. Stop on a write failure or changed
   source; files committed earlier are retained and reported. Replacement is
   atomic per file, not across the entire batch.

Rounds repeat, each with a new check, until a round applies nothing or four
rounds have run. A later round picks up fixes that conflicted with an applied
one, and fixes that the applied ones make possible. After the last write, run
one check-only validation, including when the write limit is reached or a batch
partially fails. When no write followed the last check, reuse it. The remaining
diagnostic counts therefore describe the final checked contents.

## Output

The Command Result on stdout lists `path:line:column: title` for each applied
fix, with paths relative to the current directory. `--diff` instead prints a
unified diff of the first round without applying fixes; later rounds may produce
additional changes that are absent from this preview. A summary of applied and
skipped fixes is a status line on stderr; `--verbose` explains each skipped fix of the
last valid check. Exit status 0 means the fix workflow completed, not that all
compiler diagnostics were resolved. Execution and source-write failures return
nonzero, including after earlier rounds successfully changed files. Run
`moon check` to see the remaining diagnostics. `--dry-run` prints the check
commands and applies nothing.

## Experimental limitation

n2 distinguishes failure from interruption, but does not expose the specific
exit code or most terminating signals. A compiler that emits an error diagnostic
and then crashes can therefore look like an ordinary diagnostic failure. The
current driver cannot reliably reject that case. In particular, a shell waiting
for a compiler can turn a signal sent only to that child into an ordinary exit
code; n2 then reports failure rather than interruption. Retaining structured
task results does not remove this limitation.
