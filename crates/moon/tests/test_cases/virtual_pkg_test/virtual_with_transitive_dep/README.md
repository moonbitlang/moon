# Virtual Package with Transitive Dependency Test

This test case verifies that `moon test` correctly handles virtual packages that have transitive dependencies.

## Structure

- `main` - Main package with tests, depends on `middle` and `virtual`, overrides `virtual` with `impl`
- `middle` - Intermediate package that depends on `virtual`
- `virtual` - Virtual package interface (has no implementation itself)
- `impl` - Implementation of `virtual` that depends on `dep`
- `dep` - Transitive dependency

## What this tests

Before the fix, the DFS used by `moon test` didn't properly traverse into virtual package implementations,
causing it to miss transitive dependencies like `dep`. This would result in errors like:

```
Error: Moonc.Basic_hash_string.Key_not_found("virtual_with_transitive_dep/dep")
```

After the fix, the DFS correctly resolves `virtual` to `impl` and includes all transitive dependencies.
