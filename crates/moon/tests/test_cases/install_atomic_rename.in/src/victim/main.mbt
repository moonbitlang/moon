///|
async fn main {
  println("baseline")
  if @env.args().length() > 1 {
    let self_path = @env.args()[0]
    for {
      @async.sleep(100)
      let _ = (@process.run(self_path, []) : Int)
    }
  }
}
