pub fn web_banner() -> String {
  "\{@shared.shared_banner()}-\{@lib.backend_name()}"
}

#cfg(target="js")
fn main {
  ignore(web_banner())
}
