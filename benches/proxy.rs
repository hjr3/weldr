#![feature(test)]

extern crate env_logger;
#[macro_use]
extern crate log;
extern crate futures;
extern crate tokio_core;
extern crate hyper;
extern crate weldr;
extern crate reqwest;
extern crate test;

use std::net::SocketAddr;
use std::sync::mpsc::channel;
use std::thread;

use hyper::Post;
use hyper::server::{Server, Service, Request, Response};
use hyper::header::ContentLength;

use weldr::pool::Pool;

use test::Bencher;

#[derive(Clone, Copy)]
struct Origin;

impl Service for Origin {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = ::futures::Finished<Response, hyper::Error>;

    fn call(&self, req: Request) -> Self::Future {
        ::futures::finished(match (req.method(), req.path()) {
            (&Post, Some("/echo")) => {
                let mut res = Response::new();
                if let Some(len) = req.headers().get::<ContentLength>() {
                    res.headers_mut().set(len.clone());
                }
                res.with_body(req.body())
            }
            _ => {
                panic!("benchmark should not be getting a 404");
            }
        })
    }
}

/// Send a request through the proxy and get back a response.
///
/// The client request is to created via the callback. The callback provides the host so the client
/// can connect to the correct proxy.
fn with_server<R>(mut req: R)
where
    R: FnMut(String),
{
    let _ = env_logger::init();

    let pool = Pool::with_servers(vec![]);

    let addr = "127.0.0.1:0".parse::<SocketAddr>().unwrap();
    let (_, proxy) = weldr::proxy::listen(addr, pool.clone()).expect("Failed to start server");

    let (tx, rx) = channel();
    thread::spawn(move || {
        let addr = "127.0.0.1:0".parse::<SocketAddr>().unwrap();
        let (listening, server) = Server::standalone(|tokio| {
            Server::http(&addr, tokio)?.handle(|| Ok(Origin), tokio)
        }).unwrap();
        tx.send(listening).unwrap();
        server.run();
    });

    let origin = rx.recv().unwrap();
    pool.add(*origin.addr());

    req(format!("http://127.0.0.1:{}", origin.addr().port()));

    proxy.close();
    origin.close();
}


#[bench]
// This is a relative benchmark as the cost of sending the request and the cost of the response
// from the hyper origin server is included.
fn bench_server_hello_world(b: &mut Bencher) {
    with_server(|host| {

        let url = format!("{}{}", host, "/echo");
        let client = reqwest::Client::new().expect("client failed to construct");

        b.iter(|| {
            let res = client.post(&url).body("hello").send().unwrap();

            res
        })
    });
}
