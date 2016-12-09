extern crate hyper;
extern crate alacrity;
extern crate env_logger;
#[macro_use] extern crate log;

use hyper::server::{Server, Request, Response, Handler};
use hyper::header::{ContentLength};
use hyper::Client;
use std::io::{Read, Write};
use std::net::SocketAddr;
use alacrity::pool::Pool;
use std::{thread, time};


fn with_server<H: Handler + 'static, R>(handle: H, proxy_port: u16, test: &Fn(u16) -> R) -> R {
    let mut server = Server::http("127.0.0.1:0").unwrap().handle(handle).unwrap();
    let port = server.socket.port();
    let server_addr = server.socket;

    // test directly against http server
    test(port);

    // test against proxy
    thread::spawn(move || {
        // TODO: it would be more convenient to use the port 0 to let the kernel pick one free port for us
        // https://github.com/hjr3/alacrity/issues/12
        let proxy_addr = format!("127.0.0.1:{}", proxy_port);
        let addr = proxy_addr.parse::<SocketAddr>().unwrap();
        let pool = Pool::with_servers(vec![server_addr]);
        alacrity::proxy::listen(addr, pool.clone()).expect("Failed to start server");
    });
    // TODO: need a better way to wait for proxy to be up
    thread::sleep(time::Duration::from_millis(50));

    let result_proxy = test(proxy_port);

    // TODO: close proxy - https://github.com/hjr3/alacrity/issues/11
    // TODO: close previously created thread
    server.close().unwrap();
    result_proxy
}

fn url(port: u16) -> String {
    format!("http://localhost:{}", port)
}


#[test]
fn method_on_http_server() {
    use hyper::method::Method;
    use std::str::FromStr;

    fn hello_request_method(req: Request, mut res: Response) {
        let body = format!("hello {}", req.method);
        res.headers_mut().set(ContentLength(body.len() as u64));
        let mut res = res.start().unwrap();
        res.write_all(body.as_ref()).unwrap();
    }

    let _ = env_logger::init();

    let methods = vec!("GET", "DELETE", "PATCH", "OPTIONS", "POST", "PUT", "TRACE", "HEAD", "CONNECT", "GARBAGE");
    let port = 8081;

    for (i, method) in methods.iter().enumerate() {
        with_server(hello_request_method, port + i as u16, &|port| {
            let client = Client::new();
            let url = url(port);
            let mut res = client.request(Method::from_str(&method).unwrap(), &url).send().unwrap();
            assert_eq!(res.status, hyper::Ok);

            // missing HEAD, CONNECT as the server cannot return any body
            if *method != "HEAD" && *method != "CONNECT" {
                let mut body = String::new();
                res.read_to_string(&mut body).unwrap();
                let expected = format!("hello {}", method);
                assert_eq!(body, expected);
            }
        });
    };
}

#[test]
#[ignore]
fn request_body() {
    fn hello_request_body(mut req: Request, mut res: Response) {
        let mut body = String::new();
        req.read_to_string(&mut body).unwrap();
        res.headers_mut().set(ContentLength(body.len() as u64));
        let mut res = res.start().unwrap();
        res.write_all(body.as_ref()).unwrap();
    }

    let _ = env_logger::init();

    with_server(hello_request_body, 8101, &|port| {
        let client = Client::new();
        let url = url(port);
        let mut res = client.post(&url).body("hello").send().unwrap();
        assert_eq!(res.status, hyper::Ok);

        let mut body = String::new();
        res.read_to_string(&mut body).unwrap();
        assert_eq!(body, "hello");
    });
}
