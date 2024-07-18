// Copyright 2024 International Digital Economy Academy
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
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use std::io::Error as IoError;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use colored::Colorize;
use http::response::Builder as ResponseBuilder;
use http::{header, StatusCode};
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_staticfile::{Body, Static};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;

async fn handle_request<B>(req: Request<B>, static_: Static) -> Result<Response<Body>, IoError> {
    if req.uri().path() == "/" {
        let res = ResponseBuilder::new()
            .status(StatusCode::MOVED_PERMANENTLY)
            .header(header::LOCATION, "/index.html#/")
            .body(Body::Empty)
            .expect("unable to build response");
        Ok(res)
    } else {
        static_.clone().serve(req).await
    }
}

pub fn start_server(
    root_dir: impl Into<PathBuf>,
    cake_full_name: &str,
    bind: String,
    port: u16,
) -> anyhow::Result<()> {
    let runtime = Runtime::new()?;
    runtime.block_on(async {
        let static_ = Static::new(root_dir);

        let addr = format!("{}:{}", bind, port)
            .parse::<SocketAddr>()
            .context(format!("failed to parse address {}:{}", bind, port))?;

        let listener = TcpListener::bind(addr)
            .await
            .context(format!("failed to bind to address {}", addr))?;

        eprintln!(
            "{}",
            format!(
                "Doc server running on http://{}/index.html#/{}/",
                addr, cake_full_name
            )
            .bold()
            .green()
        );
        loop {
            let (stream, _) = listener
                .accept()
                .await
                .expect("Failed to accept TCP connection");

            let static_ = static_.clone();
            tokio::spawn(async move {
                if let Err(err) = hyper::server::conn::http1::Builder::new()
                    .serve_connection(
                        TokioIo::new(stream),
                        service_fn(move |req| handle_request(req, static_.clone())),
                    )
                    .await
                {
                    eprintln!("Error serving connection: {:?}", err);
                }
            });
        }
    })
}
