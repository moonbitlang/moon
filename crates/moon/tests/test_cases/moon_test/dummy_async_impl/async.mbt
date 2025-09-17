///|
struct TaskGroup[X] {
  mut result : X?
}

///|
suberror AlreadyTerminated

///|
suberror Unfinished

///|
pub fn[X] TaskGroup::spawn_bg(
  self : TaskGroup[X],
  f : async () -> Unit,
) -> Unit raise {
  if self.result is Some(_) {
    raise AlreadyTerminated
  }
  let mut done = false
  let mut err = None
  run_async(fn() {
    f() catch { e => err = Some(e) } 
    done = true
  })
  if err is Some(err) {
    raise err
  } else if not(done) {
    raise Unfinished
  }
}

///|
pub async fn[X] with_task_group(
  f : async (TaskGroup[X]) -> X,
) -> X raise {
  let group = { result: None }
  group.result = Some(f(group))
  guard group.result is Some(result)
  result
}

///|
pub fn run_async_main(f : async () -> Unit) -> Unit {
  let mut done = false
  run_async(fn() {
    f() catch { _ => () } 
    done = true
  })
  if not(done) {
    panic()
  }
}

///|
fn run_async(f : async () -> Unit noraise) -> Unit = "%async.run"

///|
pub async fn[T] suspend(f : ((T) -> Unit) -> Unit) -> T noraise = "%async.suspend"
