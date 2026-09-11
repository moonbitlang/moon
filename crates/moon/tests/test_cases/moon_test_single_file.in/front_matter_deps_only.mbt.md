---
moonbit:
  deps:
    moonbitlang/async: 0.21.2
  backend:
    native
---

```moonbit
async fn use_import_all() -> Unit {
  let _ = @aqueue.Queue(kind=Unbounded)
}
```
