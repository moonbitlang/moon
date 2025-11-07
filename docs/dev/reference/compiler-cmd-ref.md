# Compiler Commands Reference

## `moonc build-interface`

Generate a `.mi` interface file for a virtual package from its `.mbti` contract. The build graph uses this command whenever it needs to materialise the interface for a virtual package before validating or linking implementations. The invocation mirrors [`gen_build.rs::gen_build_interface_command()`](crates/moonbuild/src/gen/gen_build.rs:369).

### Invocation

```bash
moonc build-interface <input.mbti> \
  -o <output.mi> \
  [-i <dep.mi:alias> ...] \
  -pkg <module/package> \
  -pkg-sources <module/package:/absolute/source/dir> \
  -virtual \
  [-std-path <core-bundle.mbc>] \
  -error-format=json
```

### Argument reference

| Flag / Position           | Description                                                                                                                                                                                                                                  |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `<input.mbti>`            | Absolute path to the `.mbti` interface that defines the virtual package surface.                                                                                                                                                             |
| `-o <output.mi>`          | Destination path for the generated `.mi`. The build places this next to the package’s other artifacts.                                                                                                                                       |
| `-i <dep.mi:alias>`       | Zero or more interface dependencies. Each entry must follow `path:alias`. The alias comes from the import declaration; Moonfalling back to the package’s last path component if no alias was supplied. Repeat the flag for every dependency. |
| `-pkg <module/package>`   | Fully-qualified package name (module name plus relative package path). This drives diagnostics and metadata in the emitted `.mi`.                                                                                                            |
| `-pkg-sources <pkg:dir>`  | Associates the compiled package name with its absolute source directory. Required so `moonc` can map diagnostic file locations back to disk.                                                                                                 |
| `-virtual`                | Instructs `moonc` to treat the input as a virtual package contract and to emit interface metadata only.                                                                                                                                      |
| `-std-path <core bundle>` | Optional. Supplied automatically when the build runs with the standard library enabled; points to the bundled core artifact so the interface can resolve std symbols. Omitted when `--nostd` is active.                                      |
| `-error-format=json`      | Forces structured diagnostics so the build driver can surface errors deterministically.                                                                                                                                                      |
