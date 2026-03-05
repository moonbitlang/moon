pub fn server_banner() -> String {
  "\{@shared.shared_banner()}-\{@lib.backend_name()}"
}

#cfg(target="native")
fn main {
  ignore(server_banner())
}
