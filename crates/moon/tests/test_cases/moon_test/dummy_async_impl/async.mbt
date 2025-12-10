///|
priv enum CoroutineState {
  Running
  Suspend(cont~ : (Unit) -> Unit)
  Done
  Fail(Error)
}

///|
priv struct Coroutine {
  mut state : CoroutineState
}

///|
let scheduler : @deque.Deque[Coroutine] = @deque.new()

///|
let curr_coro : Ref[Coroutine?] = @ref.new(None)

///|
fn run_forever() -> Unit {
  while scheduler.pop_front() is Some(coro) {
    match coro.state {
      Running | Done | Fail(_) => ()
      Suspend(cont~) =>{
        curr_coro.val = Some(coro)
        coro.state = Running
        cont(())
      }
    }
  }
}

///|
fn spawn_coro(f : async () -> Unit) -> Unit {
  let coro = { state: Running }
  fn worker(_) {
    run_async(() => {
      try {
        f() 
        coro.state = Done
      } catch {
        err => coro.state = Fail(err)
      }
    })
  }
  coro.state = Suspend(cont=worker)
  scheduler.push_back(coro)
}

///|
pub async fn pause() -> Unit noraise {
  guard curr_coro.val is Some(coro)
  guard coro.state is Running
  suspend(cont => {
    coro.state = Suspend(cont~)
    scheduler.push_back(coro)
  })
}

///|
struct TaskGroup[X] {
  mut result : X?
  mut err : Error?
}

///|
pub fn[X] TaskGroup::spawn_bg(
  self : TaskGroup[X],
  f : async () -> Unit,
) -> Unit {
  guard not(self.result is Some(_))
  spawn_coro(() => {
    try f() catch {
      err => {
        self.err = Some(err)
        raise err
      }
    }
  })
}

///|
pub async fn[X] with_task_group(
  f : async (TaskGroup[X]) -> X,
) -> X raise {
  let group = { result: None, err: None }
  group.spawn_bg(() => group.result = Some(f(group)))
  run_forever()
  if group.err is Some(err) {
    raise err
  }
  guard group.result is Some(result)
  result
}

///|
pub fn is_being_cancelled() -> Bool {
  false
}

///|
pub fn run_async_main(f : async () -> Unit) -> Unit {
  run_async(fn() {
    f() catch { _ => () } 
  })
  run_forever()
}

///|
fn run_async(f : async () -> Unit noraise) -> Unit = "%async.run"

///|
async fn[T] suspend(f : ((T) -> Unit) -> Unit) -> T noraise = "%async.suspend"
