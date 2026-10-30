---
moonbit:
  import:
    - path: moonbitlang/x@0.5.5/stack
      alias: xstack
---

```moonbit
fn use_stack() -> Unit {
  let _ : @xstack.Stack[Int] = @xstack.Stack::new()
}
```
