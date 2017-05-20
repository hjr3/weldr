extern crate env_logger;
#[macro_use] extern crate log;
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate hyper;
extern crate weldr;

use std::net::SocketAddr;
use std::sync::mpsc::channel;
use std::thread;
use std::str::FromStr;

use futures::{future, Future, Stream};
use tokio_core::net::{TcpListener, TcpStream};
use tokio_core::reactor::{Core, Handle};
use tokio_io::io;

use hyper::{Get, Post, StatusCode, Method, HttpVersion, Headers, Uri};
use hyper::client;
use hyper::server::{Http, Service, Request, Response};
use hyper::header::{ContentLength, TransferEncoding};

use weldr::server::Server;
use weldr::pool::Pool;

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

struct SimpleResponse {
    pub status: StatusCode,
    pub headers: Headers,
    pub version: HttpVersion,
    pub body: Option<String>,
}

impl SimpleResponse {
    pub fn from_hyper_response(response: &client::Response) -> SimpleResponse {
        SimpleResponse {
            status: response.status().clone(),
            headers: response.headers().clone(),
            version: response.version().clone(),
            body: None,
        }
    }

    pub fn set_body(&mut self, body: String) {
        self.body = Some(body);
    }

}

/// Send a request through the proxy and get back a response.
///
/// The client request is to created via the callback. The callback provides the host so the client
/// can connect to the correct proxy.
fn with_server<R> (req: R) where R: Fn(String, Handle) -> Box<Future<Item=(), Error=hyper::Error>>
{
    let _ = env_logger::init();

    let pool = Pool::default();

    let addr = "127.0.0.1:0".parse::<SocketAddr>().unwrap();
    let core = Core::new().unwrap();
    let handle = core.handle();

    let listener = TcpListener::bind(&addr, &handle).unwrap();
    let proxy_addr = listener.local_addr().unwrap();

    let _admin_listener = TcpListener::bind(&addr, &handle).unwrap();

    let (tx, rx) = channel();
    let _h2 = thread::spawn(move || {
        let addr = "127.0.0.1:0".parse::<SocketAddr>().unwrap();


        let server = Http::new().bind(&addr, || Ok(Origin)).unwrap();
        tx.send(server.local_addr().unwrap()).unwrap();
        server.run().unwrap();
    });

    let origin = rx.recv().unwrap();
    let origin_str = format!("http://127.0.0.1:{}", origin.port());
    let origin_url = origin_str.parse::<Uri>().unwrap();
    let origin_server = Server::new(origin_url, false);
    pool.add(origin_server);

    let shutdown_signal = future::lazy(|| {
        req(origin_str, handle.clone());
        req(format!("http://{}:{}", proxy_addr.ip(), proxy_addr.port()), handle.clone())
    });

    weldr::proxy::run_with(
        core,
        listener,
        pool.clone(),
        shutdown_signal).expect("Failed to start server");
}

fn client_send_request(request: client::Request, handle: &Handle)
        -> Box<Future<Item = SimpleResponse, Error = hyper::Error>>
{
    let client = client::Client::new(&handle);
    let res = client.request(request).and_then(move |res| {
        debug!("Response: {}", res.status());
        debug!("Headers: \n{}", res.headers());

        let mut s_res = SimpleResponse::from_hyper_response(&res);

        res.body().fold(Vec::new(), |mut v, chunk| {
            v.extend(&chunk[..]);
            future::ok::<_, hyper::Error>(v)
        }).and_then(move |chunks| {
            let body = String::from_utf8(chunks).unwrap();
            s_res.set_body(body);
            future::ok(s_res)
        })
    });

    Box::new(res)
}

#[test]
fn test_method_on_http_server() {
    let _ = env_logger::init();
    use std::str::FromStr;

    let methods = vec!("GET", "DELETE", "PATCH", "OPTIONS", "POST", "PUT", "TRACE", "HEAD", "CONNECT");
    for method in methods.iter() {
        with_server(|host, handle| {

            let method = method.clone();
            let h_method = Method::from_str(&method).unwrap();
            let url = hyper::Uri::from_str(&format!("{}{}", host, "/method")).unwrap();
            let req = client::Request::new(h_method, url);
            let work = client_send_request(req, &handle).and_then(move |res| {

                assert_eq!(res.status, hyper::StatusCode::Ok);

                // TODO: fix body being returned here
                // HEAD, CONNECT cannot return any body
                //if method != "HEAD" && method != "CONNECT" {
                    let expected = format!("hello {}", method);
                    assert_eq!(expected, res.body.unwrap());
                //} else {
                //    assert_eq!("", &s);
                //}

                future::ok(())
            });

            Box::new(work)
        })
    }
}

#[test]
fn test_request_body() {
    with_server(|host, handle| {

        let url = format!("{}{}", host, "/echo");
        let url = hyper::Uri::from_str(&url).unwrap();
        let mut req = client::Request::new(Method::Post, url);
        req.set_body("hello");
        let work = client_send_request(req, &handle).and_then(move |res| {

            assert_eq!(res.status, hyper::StatusCode::Ok);
            assert_eq!(res.body.unwrap(), "hello");

            future::ok(())
        });

        Box::new(work)
    })
}

#[test]
fn test_request_and_response_body_chunked() {
    // hyper client does not currently support chunked requests
    with_server(|host, handle| {

        // strip out the "http://"
        let addr: SocketAddr = host[7..].parse().unwrap();
        let tcp = TcpStream::connect(&addr, &handle);
        let req = tcp.and_then(|stream| {

            io::write_all(stream, &b"\
            POST /echo HTTP/1.1\r\n\
            Host: www.example.com\r\n\
            Accept: */*\r\n\
            Connection: close\r\n\
            Transfer-Encoding: chunked\r\n\
            \r\n\
            5\r\n\
            hello\r\n\
            0\r\n\r\n\
            "[..]).and_then(|(stream, _)| {
                io::read_to_end(stream, Vec::new())
            }).and_then(|(_, body)| {
                let body = String::from_utf8(body).unwrap();
                let n = body.find("\r\n\r\n").unwrap() + 4;
                assert_eq!(&body[n..], "5\r\nhello\r\n0\r\n\r\n");

                future::ok(())
            })
        }).map_err(From::from);

        Box::new(req)
    })
}

#[test]
fn test_response_body_streaming() {

    with_server(|host, handle| {

        let url = format!("{}{}", host, "/chunked");
        let url = hyper::Uri::from_str(&url).unwrap();
        let req = client::Request::new(Method::Get, url);
        let work = client_send_request(req, &handle).and_then(move |res| {

            assert_eq!(res.status, hyper::StatusCode::Ok);
            assert_eq!(res.body.unwrap(), "Hello Chunky World!");

            assert_eq!(
                res.headers.get::<hyper::header::TransferEncoding>(),
                Some(&hyper::header::TransferEncoding(vec![hyper::header::Encoding::Chunked]))
            );

            future::ok(())
        });

        Box::new(work)
    })
}
