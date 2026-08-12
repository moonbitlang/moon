// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::{
    collections::HashMap,
    ffi::OsString,
    io::{Cursor, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

use sha2::{Digest, Sha256};

/// An isolated registry that exercises the same Git smart HTTP and package
/// download paths as the public registry without depending on the network.
pub struct TestRegistry {
    _root: tempfile::TempDir,
    moon_home: PathBuf,
    package_routes: Vec<String>,
    requests: Arc<Mutex<Vec<String>>>,
    shutdown: Arc<AtomicBool>,
    address: SocketAddr,
    server: Option<JoinHandle<()>>,
}

/// A package published by [`TestRegistry`].
pub struct TestPackage<'a> {
    name: &'a str,
    version: &'a str,
    dependencies: &'a [(&'a str, &'a str)],
    files: &'a [(&'a str, &'a [u8])],
}

impl<'a> TestPackage<'a> {
    pub fn new(name: &'a str, version: &'a str, files: &'a [(&'a str, &'a [u8])]) -> Self {
        Self {
            name,
            version,
            dependencies: &[],
            files,
        }
    }

    pub fn with_dependencies(mut self, dependencies: &'a [(&'a str, &'a str)]) -> Self {
        self.dependencies = dependencies;
        self
    }
}

impl TestRegistry {
    pub fn new(name: &str, version: &str, files: &[(&str, &[u8])]) -> Self {
        Self::with_packages(&[TestPackage::new(name, version, files)])
    }

    pub fn empty() -> Self {
        Self::with_packages(&[])
    }

    pub fn with_packages(packages: &[TestPackage<'_>]) -> Self {
        let root = tempfile::tempdir().expect("create test registry root");
        let index_source = root.path().join("index-source");
        run_git(
            Command::new("git")
                .args(["init", "--quiet", "--initial-branch=main"])
                .arg(&index_source),
        );
        std::fs::write(index_source.join("index-version"), "1\n").unwrap();

        let mut routes = HashMap::new();
        let mut package_routes = Vec::new();
        for package in packages {
            let package_archive = zip_files(package.files);
            let checksum = format!("{:x}", Sha256::digest(&package_archive));
            let (username, unqualified_name) = package
                .name
                .split_once('/')
                .expect("registry package name contains a username");
            let index_file = index_source
                .join("user")
                .join(username)
                .join(format!("{unqualified_name}.index"));
            std::fs::create_dir_all(index_file.parent().unwrap()).unwrap();
            let dependencies = package
                .dependencies
                .iter()
                .map(|(name, version)| ((*name).to_owned(), serde_json::json!(version)))
                .collect::<serde_json::Map<_, _>>();
            let index_entry = serde_json::json!({
                "name": package.name,
                "version": package.version,
                "deps": dependencies,
                "checksum": checksum,
            });
            std::fs::write(index_file, format!("{index_entry}\n")).unwrap();

            let package_route = format!(
                "/user/{username}%2F{unqualified_name}%2F{}.zip",
                package.version
            );
            routes.insert(package_route.clone(), package_archive);
            package_routes.push(package_route);
        }
        run_git(
            Command::new("git")
                .arg("-C")
                .arg(&index_source)
                .args(["add", "."]),
        );
        run_git(Command::new("git").arg("-C").arg(&index_source).args([
            "-c",
            "user.name=Moon Test",
            "-c",
            "user.email=moon-test@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "Add fixture package",
        ]));

        let registry_root = root.path().join("registry");
        let bare_index = registry_root.join("git/index");
        std::fs::create_dir_all(bare_index.parent().unwrap()).unwrap();
        run_git(
            Command::new("git")
                .args(["clone", "--quiet", "--bare"])
                .arg(&index_source)
                .arg(&bare_index),
        );

        routes.insert(
            "/symbols.zip".to_owned(),
            zip_files(&[("symbol.txt", b"test symbol")]),
        );
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test registry server");
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let server_requests = Arc::clone(&requests);
        let server_shutdown = Arc::clone(&shutdown);
        let server = std::thread::spawn(move || {
            while let Ok((stream, _)) = listener.accept() {
                if server_shutdown.load(Ordering::Acquire) {
                    break;
                }
                handle_request(stream, &registry_root, &routes, &server_requests)
                    .expect("serve registry request");
            }
        });

        let moon_home = root.path().join("moon-home");
        std::fs::create_dir_all(&moon_home).unwrap();
        let base_url = format!("http://{address}");
        std::fs::write(
            moon_home.join("config.json"),
            serde_json::json!({
                "registry": base_url,
                "index": format!("{base_url}/git/index"),
                "symbols": format!("{base_url}/symbols.zip"),
            })
            .to_string(),
        )
        .unwrap();

        Self {
            _root: root,
            moon_home,
            package_routes,
            requests,
            shutdown,
            address,
            server: Some(server),
        }
    }

    pub fn envs(&self) -> [(&'static str, OsString); 3] {
        [
            ("MOON_HOME", self.moon_home.as_os_str().to_owned()),
            ("NO_PROXY", "127.0.0.1,localhost".into()),
            ("no_proxy", "127.0.0.1,localhost".into()),
        ]
    }

    pub fn moon_home(&self) -> &Path {
        &self.moon_home
    }

    /// Verify that the E2E did not silently fall back to a cache or a local
    /// filesystem Git transport.
    pub fn assert_used(&self) {
        let requests = self.requests.lock().unwrap();
        assert!(
            requests
                .iter()
                .any(|request| request == "GET /git/index/info/refs?service=git-upload-pack"),
            "registry index was not discovered through Git smart HTTP: {requests:?}"
        );
        assert!(
            requests
                .iter()
                .any(|request| request == "POST /git/index/git-upload-pack"),
            "registry index was not fetched through Git smart HTTP: {requests:?}"
        );
        for package_route in &self.package_routes {
            assert!(
                requests
                    .iter()
                    .any(|request| request == &format!("GET {package_route}")),
                "registry package was not downloaded: {requests:?}"
            );
        }
        assert!(
            requests.iter().any(|request| request == "GET /symbols.zip"),
            "registry symbols were not downloaded: {requests:?}"
        );
        assert!(
            self.moon_home.join("registry/index/.git/shallow").is_file(),
            "registry index clone was not shallow"
        );
    }
}

impl Drop for TestRegistry {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.address);
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

fn zip_files(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (path, contents) in files {
        archive
            .start_file(*path, zip::write::FileOptions::default())
            .unwrap();
        archive.write_all(contents).unwrap();
    }
    archive.finish().unwrap().into_inner()
}

fn run_git(command: &mut Command) {
    let output = command.output().expect("run Git for test registry");
    assert!(
        output.status.success(),
        "Git failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

struct HttpRequest {
    method: String,
    target: String,
    content_type: String,
    body: Vec<u8>,
}

fn handle_request(
    mut stream: TcpStream,
    registry_root: &Path,
    routes: &HashMap<String, Vec<u8>>,
    requests: &Mutex<Vec<String>>,
) -> std::io::Result<()> {
    let request = read_request(&mut stream)?;
    requests
        .lock()
        .unwrap()
        .push(format!("{} {}", request.method, request.target));
    let (path, query) = request
        .target
        .split_once('?')
        .map(|(path, query)| (path.to_owned(), query.to_owned()))
        .unwrap_or_else(|| (request.target.clone(), String::new()));

    if path.starts_with("/git/") {
        return serve_git_http_backend(stream, registry_root, &path, &query, request);
    }
    if let Some(body) = routes.get(&path) {
        return write_response(
            stream,
            "200 OK",
            &[("Content-Type".to_owned(), "application/zip".to_owned())],
            body,
        );
    }
    write_response(stream, "404 Not Found", &[], b"")
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<HttpRequest> {
    let mut data = Vec::new();
    let header_end = loop {
        let mut buffer = [0_u8; 8192];
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "request ended before its headers",
            ));
        }
        data.extend_from_slice(&buffer[..read]);
        if let Some(end) = data.windows(4).position(|window| window == b"\r\n\r\n") {
            break end + 4;
        }
        if data.len() > 64 * 1024 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "request headers are too large",
            ));
        }
    };

    let headers = std::str::from_utf8(&data[..header_end - 4]).map_err(std::io::Error::other)?;
    let mut lines = headers.split("\r\n");
    let mut request_line = lines.next().unwrap_or_default().split_whitespace();
    let method = request_line.next().unwrap_or_default().to_owned();
    let target = request_line.next().unwrap_or_default().to_owned();
    let mut content_length = 0;
    let mut content_type = String::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value.trim().parse().map_err(std::io::Error::other)?;
        } else if name.eq_ignore_ascii_case("content-type") {
            content_type = value.trim().to_owned();
        }
    }

    let mut body = data[header_end..].to_vec();
    while body.len() < content_length {
        let mut buffer = [0_u8; 8192];
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "request ended before its body",
            ));
        }
        body.extend_from_slice(&buffer[..read]);
    }
    body.truncate(content_length);

    Ok(HttpRequest {
        method,
        target,
        content_type,
        body,
    })
}

fn serve_git_http_backend(
    stream: TcpStream,
    registry_root: &Path,
    path: &str,
    query: &str,
    request: HttpRequest,
) -> std::io::Result<()> {
    let mut child = Command::new("git")
        .arg("http-backend")
        .env("GIT_PROJECT_ROOT", registry_root)
        .env("GIT_HTTP_EXPORT_ALL", "1")
        .env("PATH_INFO", path)
        .env("QUERY_STRING", query)
        .env("REQUEST_METHOD", &request.method)
        .env("CONTENT_TYPE", &request.content_type)
        .env("CONTENT_LENGTH", request.body.len().to_string())
        .env("SERVER_PROTOCOL", "HTTP/1.1")
        .env("REMOTE_ADDR", "127.0.0.1")
        .env("GATEWAY_INTERFACE", "CGI/1.1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(&request.body)?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "git http-backend failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let (raw_headers, body) = split_cgi_response(&output.stdout)?;
    let mut status = "200 OK".to_owned();
    let headers = raw_headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .filter_map(|(name, value)| {
            if name.eq_ignore_ascii_case("status") {
                status = value.trim().to_owned();
                None
            } else if name.eq_ignore_ascii_case("content-length")
                || name.eq_ignore_ascii_case("connection")
            {
                None
            } else {
                Some((name.to_owned(), value.trim().to_owned()))
            }
        })
        .collect::<Vec<_>>();
    write_response(stream, &status, &headers, body)
}

fn split_cgi_response(response: &[u8]) -> std::io::Result<(String, &[u8])> {
    let (header_end, separator_len) = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| (position, 4))
        .or_else(|| {
            response
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|position| (position, 2))
        })
        .ok_or_else(|| std::io::Error::other("git http-backend returned no headers"))?;
    let headers = String::from_utf8(response[..header_end].to_vec())
        .map_err(std::io::Error::other)?
        .replace('\r', "");
    Ok((headers, &response[header_end + separator_len..]))
}

fn write_response(
    mut stream: TcpStream,
    status: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> std::io::Result<()> {
    write!(stream, "HTTP/1.1 {status}\r\n")?;
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n")?;
    }
    write!(
        stream,
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)
}
