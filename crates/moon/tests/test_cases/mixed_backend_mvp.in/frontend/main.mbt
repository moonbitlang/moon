using @html {button, div, h1, p}

struct Model {
  count : Int
}

enum Msg {
  Inc
  Dec
}

fn app() -> @rabbita.Cell {
  @rabbita.simple_cell(
    model={ count: 0 },
    update=(msg, model) => {
      match msg {
        Inc => { count: model.count + 1 }
        Dec => { count: model.count - 1 }
      }
    },
    view=(dispatch, model) => {
      div(style=["padding: 16px", "font-family: sans-serif"], [
        h1(@shared.app_title()),
        p("Server endpoint: \{@shared.api_url()}"),
        p("Counter: \{model.count}"),
        div(style=["display: flex", "gap: 8px"], [
          button(on_click=dispatch(Inc), "+"),
          button(on_click=dispatch(Dec), "-"),
        ]),
      ])
    },
  )
}

pub fn mount_id() -> String {
  "app"
}

pub fn frontend_banner() -> String {
  @shared.app_title()
}

#cfg(target="js")
fn main {
  @rabbita.new(app()).mount(mount_id())
}
