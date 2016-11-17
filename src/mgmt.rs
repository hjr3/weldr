use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use rustful::{Server, Context, Response, TreeRouter};
use rustc_serialize::json;

use pool::Pool;

#[derive(Debug, RustcDecodable, RustcEncodable)]
struct PoolServers {
    pub servers: Vec<PoolServer>,
}

#[derive(Debug, RustcDecodable, RustcEncodable)]
struct PoolServer {
    pub ip: String,
    pub port: String,
}

fn index(_context: Context, response: Response) {
    response.send("Alacrity Management API");
}

fn get_servers(context: Context, response: Response) {
    let pool: &Pool = context.global.get().expect("Failed to get global pool");
    let addr = pool.get().expect("Failed to get address from pool");
    let ip = match addr.ip() {
        IpAddr::V4(v4) => format!("{}", v4),
        _ => unimplemented!(),
    };
    let servers = vec![
        PoolServer {
            ip: ip,
            port: format!("{}", addr.port()),
        }
    ];

    response.send(json::encode(&servers).expect("Failed to encode into json"))
}

fn add_server(mut context: Context, response: Response) {

    let server: PoolServer = context.body.decode_json_body().expect("Failed to decode body");
    debug!("body = {:?}", server);

    let pool: &Pool = context.global.get().expect("Failed to get global pool");
    let ip = format!("{}:{}", server.ip, server.port);
    pool.add(FromStr::from_str(&ip).expect("Failed to parse socket addr"));
    debug!("Added new IP to pool");

    // TODO send the entire pool in response
    response.send("Added new IP to pool");
}

fn remove_server(context: Context, response: Response) {

    let pool: &Pool = context.global.get().expect("Failed to get global pool");
    let host = context.variables.get("host").expect("Failed to get host");
    let port = context.variables.get("port").expect("Failed to get port");
    let addr = FromStr::from_str(format!("{}:{}", host, port).as_str()).expect("Failed to parse host and port");
    pool.remove(&addr);
    info!("Removed server {} from pool", &addr);

    // TODO send the entire pool in response
    response.send("Server removed");
}

pub fn listen(addr: SocketAddr, pool: Pool) {
    Server {
        host: addr.into(),
        handlers: insert_routes!{
            TreeRouter::new() => {
                "/" => Get: index as fn(Context, Response),
                "/servers" => Post: add_server as fn(Context, Response),
                "/servers" => Get: get_servers as fn(Context, Response),
                "/servers/:server_id" => Delete: remove_server as fn(Context, Response),
            }
        },
        global: Box::new(pool).into(),
        threads: Some(1),
        ..Server::default()
    }.run().expect("Could not start server");
}
