---
moonbit:
  deps:
    moonbitlang/async: 0.16.5
---

```moonbit
async fn use_import_all() -> Unit {
  let _ = @aqueue.Queue::new(kind=Unbounded)
}
```
