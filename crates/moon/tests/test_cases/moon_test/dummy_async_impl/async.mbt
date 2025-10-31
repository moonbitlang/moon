///|
struct TaskGroup[X] {
  mut result : X?
  mut err : Error?
}

///|
suberror Unfinished

///|
pub fn[X] TaskGroup::spawn_bg(
  self : TaskGroup[X],
  f : async () -> Unit,
) -> Unit {
  guard not(self.result is Some(_))
  let mut done = false
  run_async(fn() {
    f() catch { e => self.err = Some(e) } 
    done = true
  })
  guard done
}

///|
pub async fn[X] with_task_group(
  f : async (TaskGroup[X]) -> X,
) -> X raise {
  let group = { result: None, err: None }
  group.result = Some(f(group))
  if group.err is Some(err) {
    raise err
  }
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
