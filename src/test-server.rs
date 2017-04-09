#[macro_use]
extern crate rustful;

extern crate log;
extern crate env_logger;

use std::env;
use std::path::Path;
use rustful::{Server, Context, Response, TreeRouter};
use rustful::server::Host;


fn index(_context: Context, response: Response) {
    response.send("Hello World");
}

fn large(_context: Context, response: Response) {
    let pwd = env!("CARGO_MANIFEST_DIR");
    let path = format!("{}/tests/jquery-1.7.1.min.js", pwd);
    let path = Path::new(&path);

    let _ = response.send_file(path)
        .or_else(|e| e.send_not_found("the file was not found"))
        .or_else(|e| e.ignore_send_error());
}

fn launch_test_server(port: u16) {

    let threads = env::var("THREADS").ok().and_then(|t| t.parse::<usize>().ok().or(None));

    Server {
            host: Host::any_v4(port),
            handlers: insert_routes!{
            TreeRouter::new() => {
                "/" => Get: index as fn(Context, Response),
                "/large" => Get: large as fn(Context, Response),
            }
        },
            threads: threads,
            ..Server::default()
        }
        .run()
        .expect("Could not start server");
}

fn main() {

    env_logger::init().expect("Failed to init logger");

    let test_server_port = env::args().nth(1).unwrap_or("12345".to_string());
    let test_server_port: u16 = test_server_port.parse().unwrap();

    launch_test_server(test_server_port);
}

