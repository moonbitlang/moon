---
moonbit:
  import:
    - path: moonbitlang/x@0.4.38/stack
      alias: xstack
---

```moonbit
fn use_stack() -> Unit {
  let _ : @xstack.Stack[Int] = @xstack.Stack::new()
}
```
