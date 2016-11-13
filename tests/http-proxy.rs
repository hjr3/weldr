extern crate hyper;
// extern crate proxy;

use hyper::server::{Server, Request, Response, Handler};
use hyper::Client;
use std::io::Read;
use std::net::SocketAddr;


fn with_server<H: Handler + 'static, R>(handle: H, test: &Fn(u16) -> R) -> R {
    let mut server = Server::http("localhost:0").unwrap().handle(handle).unwrap();
    let port = server.socket.port();

    // test directly against http server
    let result_direct = test(port);

    // test against proxy
    let proxy_addr = "127.0.0.1:8081".to_string();
    let addr = proxy_addr.parse::<SocketAddr>().unwrap();
//    alacrity::new_proxy(addr, socket_addr(port));
//    let result_proxy = test(8081);

    server.close().unwrap();
    result_direct
}

fn url(port: u16) -> String {
    format!("http://localhost:{}", port)
}

fn socket_addr(port: u16) -> String {
    format!("127.0.0.1:{}", port)
}


#[test]
fn get_on_http_server() {
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
