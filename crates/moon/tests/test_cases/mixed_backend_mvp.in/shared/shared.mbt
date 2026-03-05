pub fn app_title() -> String {
  "MoonBuild Mixed Backend MVP"
}

pub fn server_addr() -> String {
  "127.0.0.1:3001"
}

pub fn hello_path() -> String {
  "/api/hello"
}

pub fn hello_message() -> String {
  "Hello from MoonBit shared package"
}

pub fn api_url() -> String {
  "http://\{server_addr()}\{hello_path()}"
}
