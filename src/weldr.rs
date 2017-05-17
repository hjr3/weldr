extern crate env_logger;
extern crate hyper;
extern crate weldr;

use std::env;
use std::net::SocketAddr;
//use std::time::Duration;

use hyper::Url;

use weldr::server::Server;
use weldr::pool::Pool;
use weldr::proxy::ConfFile;



fn main() {
    env_logger::init().expect("Failed to start logger");

    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    let backend = env::args().nth(2).unwrap_or("http://127.0.0.1:12345".to_string());
    let backend = backend.parse::<Url>().unwrap();
    let map_host = env::args().nth(4).unwrap_or("false".to_string());
    let map_host = if map_host == "true" { true } else { false };
    let server = Server::new(backend, map_host);
    let pool = Pool::default();
    let _ = pool.add(server);

    let admin_ip = env::args().nth(3).unwrap_or("127.0.0.1:8687".to_string());
    let admin_addr = admin_ip.parse::<SocketAddr>().unwrap();

    let conf_file = ConfFile { timeout: 5}; // set timeout in secs
    //let p = pool.clone();
    //let _ = thread::Builder::new().name("health-check".to_string()).spawn(move || {
    //    let checker = health::HealthCheck::new(Duration::from_millis(1000), p, "/".to_owned());
    //    checker.run();
    //}).expect("Failed to create proxy thread");

    weldr::proxy::run(addr, admin_addr, pool, conf_file).expect("Failed to start server");
}
