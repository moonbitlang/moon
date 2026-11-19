---
defaults:
  output_stream: combined
  fail_fast: true
---
# An opened file keeps its grant

On Unix, replacing the opening name with a symlink does not change the opened
file's read/write or metadata permission. New opens still use the path rules.

```scrut
$ mkdir work
```

```scrut
$ printf keep > work/file
```

```scrut
$ printf private > other
```

```scrut
$ cat > example.mbtx <<'MOON'
> ----
> import {
>   "moonbitlang/async@0.22.7",
>   "moonbitlang/async@0.22.7/fs",
> }
> permission(fs: { "read": ["work"], "write": ["work"] })
> ----
> async fn main {
>   let file = @fs.open("work/file", mode=ReadWrite)
>   defer file.close()
>   @fs.rename("work/file", "work/moved")
>   @fs.symlink(target="../other", "work/file")
>   println(file.read_all().text())
>   println(file.size())
>   file.write_at(b"new!", position=0)
> }
> MOON
```

```scrut
$ MOONRUN_OVERRIDE="$TESTDIR/../../../../target/debug/moonrun" moon run example.mbtx --target wasm --quiet
keep
4
```

```scrut
$ test "$(cat work/moved)" = 'new!' && test "$(cat other)" = 'private'
```
