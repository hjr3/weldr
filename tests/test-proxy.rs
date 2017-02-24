extern crate env_logger;
#[macro_use] extern crate log;
extern crate futures;
extern crate tokio_core;
extern crate hyper;
extern crate weldr;
extern crate reqwest;

use std::io::{Read, Write};
use std::net::{TcpStream, SocketAddr};
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use hyper::{Get, Post, StatusCode};
use hyper::server::{Http, Service, Request, Response};
use hyper::header::{ContentLength, TransferEncoding};

use weldr::pool::{Pool, Server};

#[derive(Clone, Copy)]
struct Origin;

impl Service for Origin {
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
            },
            (_, "/method") => {
                let body = format!("hello {}", req.method());
                Response::new()
                    .with_header(ContentLength(body.len() as u64))
                    .with_body(body)
            },
            (&Post, "/echo") => {
                let mut res = Response::new();
                if let Some(len) = req.headers().get::<ContentLength>() {
                    res.headers_mut().set(len.clone());
                }
                res.with_body(req.body())
            },
            (_, "/chunked") => {
                Response::new()
                    .with_header(TransferEncoding::chunked())
                    .with_body("Hello Chunky World!")
            },
            _ => {
                Response::new()
                    .with_status(StatusCode::NotFound)
            }
        })
    }
}

/// Send a request through the proxy and get back a response.
///
/// The client request is to created via the callback. The callback provides the host so the client
/// can connect to the correct proxy.
fn with_server<R> (req: R) where R: Fn(String)
{
    let _ = env_logger::init();

    let pool = Pool::with_servers(vec![]);

    let addr = "127.0.0.1:0".parse::<SocketAddr>().unwrap();
    let (_h1, proxy_addr) = weldr::proxy::listen(addr, pool.clone()).expect("Failed to start server");

    let (tx, rx) = channel();
    let _h2 = thread::spawn(move || {
        let addr = "127.0.0.1:0".parse::<SocketAddr>().unwrap();


        let server = Http::new().bind(&addr, || Ok(Origin)).unwrap();
        tx.send(server.local_addr().unwrap()).unwrap();
        server.run().unwrap();
    });

    let origin = rx.recv().unwrap();
    let origin_str = format!("http://127.0.0.1:{}", origin.port());
    pool.add(origin_str.parse::<Server>().unwrap());

    req(origin_str);
    req(format!("http://{}:{}", proxy_addr.ip(), proxy_addr.port()));
}

/// Utility function for creating a raw request to the proxy
fn connect(host: &str) -> TcpStream {
    // strip out the "http://"
    let addr: SocketAddr = host[7..].parse().unwrap();
    let req = TcpStream::connect(&addr).unwrap();
    req.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    req.set_write_timeout(Some(Duration::from_secs(1))).unwrap();
    req
}

#[test]
fn test_method_on_http_server() {
    use reqwest::Method;
    use std::str::FromStr;

    let methods = vec!("GET", "DELETE", "PATCH", "OPTIONS", "POST", "PUT", "TRACE", "HEAD", "CONNECT");
    for method in methods.iter() {
        with_server(|host| {

            let h_method = Method::from_str(&method).unwrap();
            let url = format!("{}{}", host, "/method");
            let client = reqwest::Client::new().expect("client failed to construct");
            let mut res = client.request(h_method, &url).send().unwrap();

            debug!("Response: {}", res.status());
            debug!("Headers: \n{}", res.headers());

            assert_eq!(res.status(), &reqwest::StatusCode::Ok);

            // HEAD, CONNECT cannot return any body
            if *method != "HEAD" && *method != "CONNECT" {
                let mut body = String::new();
                res.read_to_string(&mut body).unwrap();
                let expected = format!("hello {}", method);
                assert_eq!(expected, body);
            } else {
                let mut body = String::new();
                res.read_to_string(&mut body).unwrap();
                assert_eq!("".to_string(), body);
            }
        });
    }
}

#[test]
fn test_request_body() {
    with_server(|host| {

        let url = format!("{}{}", host, "/echo");
        let client = reqwest::Client::new().expect("client failed to construct");
        let mut res = client.post(&url)
            .body("hello")
            .send().unwrap();

        assert_eq!(res.status(), &reqwest::StatusCode::Ok);

        let mut body = String::new();
        res.read_to_string(&mut body).unwrap();
        assert_eq!(body, "hello");
    });
}

#[test]
fn test_request_and_response_body_chunked() {
    with_server(|host| {

        // reqwest does not currently support chunked requests
        let mut req = connect(&host);
        req.write_all(b"\
            POST /echo HTTP/1.1\r\n\
            Host: www.example.com\r\n\
            Accept: */*\r\n\
            Connection: close\r\n\
            Transfer-Encoding: chunked\r\n\
            \r\n\
            5\r\n\
            hello\r\n\
            0\r\n\r\n\
        ").expect("Raw request failed");
        let mut body = String::new();
        req.read_to_string(&mut body).expect("Raw response failed");
        let n = body.find("\r\n\r\n").unwrap() + 4;

        assert_eq!(&body[n..], "5\r\nhello\r\n0\r\n\r\n");
    });
}

#[test]
fn test_response_body_streaming() {
    with_server(|host| {
        let url = format!("{}{}", host, "/chunked");
        let mut res = reqwest::get(&url).unwrap();

        assert_eq!(res.status(), &reqwest::StatusCode::Ok);

        assert_eq!(res.headers().get::<reqwest::header::TransferEncoding>(),
            Some(&reqwest::header::TransferEncoding(vec![reqwest::header::Encoding::Chunked])));

        let mut body = String::new();
        res.read_to_string(&mut body).unwrap();
        assert_eq!(body, "Hello Chunky World!");
    });
}
