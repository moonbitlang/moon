# Behavior of `moon runwasm`

> This page documents registry-backed prebuilt wasm mode of `moon runwasm`.
> Local package inputs are delegated to `moon run --target wasm`.
>
> Status: expected behavior in this branch as of June 10, 2026.

## Scope

`moon runwasm` has two modes:

1. Local package mode:
   `moon runwasm <local-package>` builds and runs the package as wasm.
2. Mooncakes asset mode:
   `moon runwasm <user/module[/package][@version]>` runs a prebuilt wasm asset
   published for a registry module version.

This document focuses on Mooncakes asset mode.

## Coordinate model

A **pinned coordinate** includes an explicit version:

- `user/module/package@1.2.3`
- `user/module@1.2.3/package`

An **unpinned coordinate** omits the version:

- `user/module/package`

Pinned coordinates use the supplied version directly. They do not need the
registry index for version resolution.

Unpinned coordinates must resolve the latest module version from the local
registry index before the asset URL and cache path can be computed.

## Registry update policy

The registry update policy optimizes the common rerun case:

1. The user runs an unpinned coordinate once.
2. The latest version has already been resolved into the local registry index.
3. The wasm asset may also already be cached.
4. Running the same command again should not spend time updating the registry
   index before using the already-resolvable version.

Rules:

- For pinned coordinates, skip registry index update.
- For unpinned coordinates, read the local registry index first.
- If the local index contains usable version metadata for the module, use that
  version and skip registry index update.
- If the local index does not contain usable version metadata for the module,
  run the registry update once, then retry local resolution.
- This is a module-level check. It does not inspect whether the requested package
  path exists in newer versions.

## Version lookup outcomes

After retrying any needed registry update, version lookup has three outcomes:

- Version found: use it to form the prebuilt wasm asset URL.
- Module found but no version metadata: fail with a no-version-information error.
- Module not found: fail with a module-not-found error.

If a registry update fails while a local index already exists, the command warns
and continues with the existing local index. If no local index exists, registry
update failure is fatal because there is no cached metadata to use.

Unreadable or corrupt local index data follows the existing registry behavior:
it is treated as not resolving a latest version, so `runwasm` attempts the
update/retry path.

## Asset cache policy

Version resolution and wasm asset caching are separate.

Once a version is resolved, `runwasm` computes the asset URL and cache path for
that exact module version and package path. A cache hit runs the cached wasm.
A cache miss downloads and verifies the wasm asset.

An asset cache miss does not trigger registry update. At that point the version
has already been resolved.
