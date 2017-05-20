extern crate futures;
#[macro_use] extern crate log;
extern crate env_logger;
#[macro_use] extern crate hyper;
extern crate hyper_tls;
extern crate serde;
extern crate serde_json;
#[macro_use] extern crate serde_derive;
extern crate tokio_core;
extern crate tokio_service;
extern crate tokio_timer;
extern crate tokio_io;
extern crate nix;
extern crate libc;
extern crate capnp;
#[macro_use] extern crate capnp_rpc;
extern crate net2;

pub mod weldr_capnp {
	include!(concat!(env!("OUT_DIR"), "/weldr_capnp.rs"));
}

pub mod server;
pub mod pool;
pub mod proxy;
pub mod mgmt;
pub mod stats;
pub mod config;
