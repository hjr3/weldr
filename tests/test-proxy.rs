extern crate hyper;
extern crate alacrity;
extern crate env_logger;
#[macro_use] extern crate log;

use hyper::server::{Server, Request, Response, Handler};
use hyper::Client;
use std::io::Read;
use std::net::SocketAddr;
use alacrity::pool::Pool;
use std::{thread, time};


fn with_server<H: Handler + 'static, R>(handle: H, test: &Fn(u16) -> R) -> R {
    let mut server = Server::http("127.0.0.1:0").unwrap().handle(handle).unwrap();
    let port = server.socket.port();
    let server_addr = server.socket;

    // test directly against http server
    test(port);

    // test against proxy
    thread::spawn(move || {
        // TODO: it would be more convenient to use the port 0 to let the kernel pick one free port for us
        // https://github.com/hjr3/alacrity/issues/12
        let proxy_addr = "127.0.0.1:8081".to_string();
        let addr = proxy_addr.parse::<SocketAddr>().unwrap();
        let pool = Pool::with_servers(vec![server_addr]);
        alacrity::proxy::listen(addr, pool.clone()).expect("Failed to start server");
    });
    // TODO: need a better way to wait for proxy to be up
    thread::sleep(time::Duration::from_millis(50));
    let result_proxy = test(8081);

    // TODO: close proxy - https://github.com/hjr3/alacrity/issues/11
    // TODO: close previously created thread
    server.close().unwrap();
    result_proxy
}

fn url(port: u16) -> String {
    format!("http://localhost:{}", port)
}

#[test]
fn get_on_http_server() {
    let _ = env_logger::init();

    fn handle(_: Request, res: Response) {
        res.send(b"hello world").unwrap();
    }

    with_server(handle, &|port| {
        let client = Client::new();
        let url = url(port);
        let mut res = client.get(&url).send().unwrap();
        assert_eq!(res.status, hyper::Ok);

        let mut body = String::new();
        res.read_to_string(&mut body).unwrap();
        assert_eq!(body, "hello world");
    });
}
