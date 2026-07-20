# Deprecate implicit postadd hooks

The `scripts.postadd` package hook is deprecated because package extraction should not implicitly execute arbitrary package commands. During migration, manifests continue to accept it and an explicit `moon add` warns before retaining compatibility execution. Automatic dependency synchronization, `moon fetch`, `moon install`, and delegated execution such as `moonx` warn and skip the hook.

There is no general `--allow-postadd` escape hatch. Once affected packages have an explicit setup or lazy-build path, hook execution and then the schema entry can be removed.
