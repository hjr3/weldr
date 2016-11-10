#[macro_use] extern crate rustful;

#[macro_use] extern crate log;
extern crate env_logger;

use std::env;
use rustful::{Server, Context, Response, TreeRouter};

fn index(_context: Context, response: Response) {
    response.send("Hello World");
}

fn main() {

    let threads = env::var("THREADS").ok().and_then(|t| {
        t.parse::<usize>().ok().or(None)
    });

    env_logger::init().expect("Failed to init logger");

    Server {
        host: 12345.into(),
        handlers: insert_routes!{
            TreeRouter::new() => {
                "/" => Get: index as fn(Context, Response),
            }
        },
        threads: threads,
        ..Server::default()
    }.run().expect("Could not start server");
}
