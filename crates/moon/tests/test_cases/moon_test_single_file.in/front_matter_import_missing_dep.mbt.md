---
moonbit:
  import:
    - path: moonbitlang/x/fs
      alias: xfs
---

```moonbit
fn use_fs() -> Unit {
  let _ = @xfs.read_file_to_string("missing.txt")
}
```
