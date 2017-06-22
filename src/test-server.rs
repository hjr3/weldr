extern crate log;
extern crate env_logger;
extern crate futures;
extern crate hyper;

use std::env;
use std::fs::File;
use std::io::{Read, BufReader};
use std::net::SocketAddr;
use std::path::Path;

use hyper::{Get, StatusCode};
use hyper::server::{Http, Service, Request, Response};
use hyper::header::{ContentLength, ContentType};

fn large() -> Response {
    let pwd = env!("CARGO_MANIFEST_DIR");
    let path = format!("{}/tests/jquery-1.7.1.min.js", pwd);
    let path = Path::new(&path);

    let file = File::open(path).expect("Failed to open file");
    let mut reader = BufReader::new(file);

    let mut body = Vec::new();
    reader.read_to_end(&mut body).expect("Failed to read file");

    // marked this as type plaintext as i do not want to import the entire mime crate for only this
    // use case
    Response::new()
        .with_header(ContentLength(body.len() as u64))
        .with_header(ContentType::plaintext())
        .with_body(body)
}

#[derive(Clone, Copy)]
struct TestServer;

impl Service for TestServer {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = ::futures::Finished<Response, hyper::Error>;

    fn call(&self, req: Request) -> Self::Future {
        ::futures::finished(match (req.method(), req.path()) {
            (&Get, "/") => {
                let body = "Hello World";
                Response::new()
                    .with_header(ContentLength(body.len() as u64))
                    .with_body(body)
            }
            (&Get, "/large") => large(),
            _ => Response::new().with_status(StatusCode::NotFound),
        })
    }
}

fn main() {
    env_logger::init().expect("Failed to init logger");

    let port = env::args().nth(1).unwrap_or("12345".to_string());

    let addr = format!("0.0.0.0:{}", port)
        .parse::<SocketAddr>()
        .expect("Failed to parse socket addr");
    let server = Http::new().bind(&addr, || Ok(TestServer)).unwrap();
    server.run().unwrap();
}
