---
moonbit:
  import:
    - path: moonbitlang/x@0.4.38/stack
      alias: xstack
---

```moonbit
fn use_missing_import() -> Unit {
  let _ : @xstack.Stack[Int] = @xstack.Stack::new()
  let _ : @crypto.MD5 = @crypto.MD5::new()
}
```
