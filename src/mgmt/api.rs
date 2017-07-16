use serde_json;

use futures::{Future, Stream};

use tokio_core::reactor::Handle;

use hyper::{self, Delete, Get, Post, StatusCode, Uri};
use hyper::server::{Service, Request, Response};
use hyper::header::{ContentLength, ContentType};

use server::Server;
use pool::Pool;
use super::manager::Manager;

// HATEOAS links: https://en.wikipedia.org/wiki/HATEOAS
#[derive(Debug, Serialize, Deserialize)]
struct Link {
    pub rel: String,
    pub href: String,
    pub method: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PoolServers {
    pub servers: Vec<PoolServer>,
    pub links: Option<Vec<Link>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PoolServer {
    pub url: String,
    pub links: Option<Vec<Link>>,
}
#[derive(Debug, Serialize, Deserialize)]
struct Index {
    pub about: String,
    pub links: Vec<Link>,
}

fn index() -> Response {
    let index = Index {
        about: "Weldr Management API".to_string(),
        links: vec![
            Link {
                rel: "servers".to_string(),
                href: "/servers".to_string(),
                method: None,
            },
        ],
    };

    let body = serde_json::to_string_pretty(&index).expect("Failed to encode into json");

    Response::new()
        .with_header(ContentLength(body.len() as u64))
        .with_header(ContentType::json())
        .with_body(body)
}

fn all_servers_reponse(pool: &Pool) -> Response {
    let backends = pool.all();
    let all_servers: Vec<Server> = backends.iter().map(|backend| backend.server()).collect();
    let servers: Vec<PoolServer> = all_servers
        .into_iter()
        .map(|server| {
            let delete_href = format!("/servers/{}", server.url());
            PoolServer {
                url: server.url().as_ref().to_string(),
                links: Some(vec![
                    Link {
                        rel: "delete".to_string(),
                        href: delete_href,
                        method: Some("DELETE".to_string()),
                    },
                ]),
            }
        })
        .collect();

    let pool_servers = PoolServers {
        servers: servers,
        links: Some(vec![
            Link {
                rel: "add".to_string(),
                href: "/servers".to_string(),
                method: Some("POST".to_string()),
            },
        ]),
    };

    let body = serde_json::to_string_pretty(&pool_servers).expect("Failed to encode into json");

    Response::new()
        .with_header(ContentLength(body.len() as u64))
        .with_header(ContentType::json())
        .with_body(body)
}

fn get_servers(pool: &Pool) -> Response {
    all_servers_reponse(pool)
}

fn add_server(
    request: Request,
    pool: Pool,
    manager: Manager,
    handle: Handle,
) -> Box<Future<Item = Response, Error = hyper::Error>> {

    let work = request
        .body().concat2().and_then(move |chunk| {
            let response = match serde_json::from_slice::<PoolServer>(&chunk) {
                Ok(server) => {
                    debug!("body = {:?}", server);

                    let backend = server
                        .url
                        .parse::<Uri>()
                        .expect("Failed to parse server url");
                    let backend = Server::new(backend, true);
                    pool.add(backend);
                    debug!("Added new server to pool");

                    let backend = server
                        .url
                        .parse::<Uri>()
                        .expect("Failed to parse server url");
                    manager.publish_new_server(backend, handle);

                    all_servers_reponse(&pool)
                }
                Err(e) => {
                    let body = format!("invalid JSON: {}", e);
                    Response::new()
                        .with_status(StatusCode::BadRequest)
                        .with_header(ContentLength(body.len() as u64))
                        .with_body(body)
                }
            };

            ::futures::finished(response)
        });

    Box::new(work)
}

// TODO figure out how to parse out query k/v pairs or parse the path
//fn remove_server(context: Context, response: Response) {
//
//    let pool: &Pool = context.global.get().expect("Failed to get global pool");
//    let url = context.variables.get("url").expect("Failed to get url");
//    match FromStr::from_str(&url) {
//        Ok(server) => {
//            pool.remove(&server);
//            info!("Removed server {:?} from pool", server);
//        }
//        _ => ()
//    };
//    all_servers_reponse(pool, response)
//}

#[derive(Debug)]
pub struct Mgmt {
    pool: Pool,
    handle: Handle,
    manager: Manager,
}

impl Mgmt {
    pub fn new(pool: Pool, handle: Handle, manager: Manager) -> Mgmt {
        Mgmt {
            pool: pool,
            handle: handle,
            manager: manager,
        }
    }
}

impl Service for Mgmt {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Response, Error = hyper::Error>>;

    fn call(&self, req: Request) -> Self::Future {
        match (req.method(), req.path()) {
            (&Get, "/") => Box::new(::futures::finished(index())),
            (&Get, "/servers") => Box::new(::futures::finished(get_servers(&self.pool))),
            (&Post, "/servers") => {
                add_server(
                    req,
                    self.pool.clone(),
                    self.manager.clone(),
                    self.handle.clone(),
                )
            }
            (&Delete, "/servers") => {
                let body = "Remove server";
                Box::new(::futures::finished(
                    Response::new()
                        .with_header(ContentLength(body.len() as u64))
                        .with_body(body),
                ))
            }
            _ => {
                Box::new(::futures::finished(
                    Response::new().with_status(StatusCode::NotFound),
                ))
            }
        }
    }
}
