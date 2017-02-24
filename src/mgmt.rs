use std::net::SocketAddr;
use std::str::FromStr;

use rustful::{Server, Context, Response, StatusCode, TreeRouter};
use rustful::header::ContentType;
use rustc_serialize::Encodable;
use rustc_serialize::json::{Encoder, EncodeResult};

use pool::Pool;

// HATEOAS links: https://en.wikipedia.org/wiki/HATEOAS
#[derive(Debug, RustcDecodable, RustcEncodable)]
struct Link {
    pub rel: String,
    pub href: String,
    pub method: Option<String>,
}

#[derive(Debug, RustcDecodable, RustcEncodable)]
struct PoolServers {
    pub servers: Vec<PoolServer>,
    pub links: Option<Vec<Link>>,
}

#[derive(Debug, RustcDecodable, RustcEncodable)]
struct PoolServer {
    pub url: String,
    pub links: Option<Vec<Link>>,
}

#[derive(Debug, RustcDecodable, RustcEncodable)]
struct Index {
    pub about: String,
    pub links: Vec<Link>,
}

fn index(_context: Context, mut response: Response) {
    response.headers_mut().set::<ContentType>(ContentType::json());
    let index = Index {
        about: "Weldr Management API".to_string(),
        links: vec![Link {
            rel: "servers".to_string(),
            href: "/servers".to_string(),
            method: None,
        }]
    };
    response.send(encode_pretty(&index).expect("Failed to encode into json"))
}

fn encode_pretty<T: Encodable>(object: &T) -> EncodeResult<String> {
    let mut s = String::new();
    {
        let mut encoder = Encoder::new_pretty(&mut s);
        try!(object.encode(&mut encoder));
    }
    Ok(s)
}

fn all_servers_reponse(pool: &Pool, mut response: Response) {
    let all_servers = pool.all();
    let servers: Vec<PoolServer> = all_servers.into_iter().map(|server| {
        let url = server.url().as_str().to_string();
        // TODO: find a better way to identify a server
        let delete_href = format!("/servers/{}", url);
        PoolServer {
            url: url,
            links: Some(vec![Link {
                rel: "delete".to_string(),
                href: delete_href,
                method: Some("DELETE".to_string()),
            }]),
        }
    }).collect();

    let pool_servers = PoolServers {
        servers: servers,
        links: Some(vec![Link {
            rel: "add".to_string(),
            href: "/servers".to_string(),
            method: Some("POST".to_string()),
        }]),
    };

    response.headers_mut().set::<ContentType>(ContentType::json());
    response.send(encode_pretty(&pool_servers).expect("Failed to encode into json"))
}

fn get_servers(context: Context, response: Response) {
    let pool: &Pool = context.global.get().expect("Failed to get global pool");
    all_servers_reponse(pool, response)
}

fn add_server(mut context: Context, mut response: Response) {

    match context.body.decode_json_body::<PoolServer>() {
        Ok(server) => {
            debug!("body = {:?}", server);

            let pool: &Pool = context.global.get().expect("Failed to get global pool");
            pool.add(FromStr::from_str(&server.url).expect("Failed to parse server url"));
            debug!("Added new server to pool");

            all_servers_reponse(pool, response)
        }
        Err(e) => {
            response.set_status(StatusCode::BadRequest);
            response.send(format!("invalid JSON: {}", e))
        }
    }
}

fn remove_server(context: Context, response: Response) {

    let pool: &Pool = context.global.get().expect("Failed to get global pool");
    let url = context.variables.get("url").expect("Failed to get url");
    match FromStr::from_str(&url) {
        Ok(server) => {
            pool.remove(&server);
            info!("Removed server {:?} from pool", server);
        }
        _ => ()
    };
    all_servers_reponse(pool, response)
}

pub fn listen(addr: SocketAddr, pool: Pool) {
    info!("Starting administration server listening on http://{}", &addr);
    Server {
        host: addr.into(),
        handlers: insert_routes!{
            TreeRouter::new() => {
                "/" => Get: index as fn(Context, Response),
                "/servers" => Post: add_server as fn(Context, Response),
                "/servers" => Get: get_servers as fn(Context, Response),
                "/servers/:host/:port" => Delete: remove_server as fn(Context, Response),
            }
        },
        global: Box::new(pool).into(),
        threads: Some(1),
        ..Server::default()
    }.run().expect("Could not start server");
}
