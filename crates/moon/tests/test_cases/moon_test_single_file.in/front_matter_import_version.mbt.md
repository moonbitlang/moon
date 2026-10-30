---
moonbit:
  import:
    - moonbitlang/async@0.21.2/aqueue
  backend:
    native
---

```mbt check
async fn use_explicit_import() -> Unit {
  let _ = @aqueue.Queue(kind=Unbounded)
}
```
