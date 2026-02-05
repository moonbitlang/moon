---
moonbit:
  import:
    - moonbitlang/async@0.16.5
    - path: moonbitlang/async/aqueue
      alias: aaqueue
  backend:
    native
---

```mbt check
async test "spawn basic" {
  let buf = StringBuilder::new()
  @async.with_task_group(fn(root) {
    root.spawn_bg(() => {
      @async.sleep(100)
      buf.write_string("task 1, tick 1\n")
      @async.sleep(200)
      buf.write_string("task 1, tick 2\n")
      @async.sleep(200)
      buf.write_string("task 1, tick 3\n")
    })
    root.spawn_bg(() => {
      @async.sleep(200)
      buf.write_string("task 2, tick 1\n")
      @async.sleep(200)
      buf.write_string("task 2, tick 2\n")
      @async.sleep(200)
      buf.write_string("task 2, tick 3\n")
    })
  })
  inspect(
    buf.to_string(),
    content=(
      #|task 1, tick 1
      #|task 2, tick 1
      #|task 1, tick 2
      #|task 2, tick 2
      #|task 1, tick 3
      #|task 2, tick 3
      #|
    ),
  )
}

async test "aqueue basic" {
  let log = StringBuilder::new()
  @async.with_task_group(fn(root) {
    let queue = @aaqueue.Queue::new(kind=Unbounded)
    root.spawn_bg(() => {
      for _ in 0..<6 {
        log.write_string("get => \{queue.get()}\n")
      }
    })
    root.spawn_bg(() => {
      for x in 0..<6 {
        @async.sleep(70)
        queue.put(x)
        log.write_string("put(\{x})\n")
      }
    })
  })
  inspect(
    log.to_string(),
    content=(
      #|put(0)
      #|get => 0
      #|put(1)
      #|get => 1
      #|put(2)
      #|get => 2
      #|put(3)
      #|get => 3
      #|put(4)
      #|get => 4
      #|put(5)
      #|get => 5
      #|
    ),
  )
}

```
