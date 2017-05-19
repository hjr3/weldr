[![Build Status](https://travis-ci.org/hjr3/weldr.svg?branch=master)](https://travis-ci.org/hjr3/weldr)

# Weldr

A HTTP 1.1 reverse proxy written in Rust using hyper (tokio version).

## Design

The goal is to build an _AWS ELB_-like reverse proxy that works well in the dynamic VM/container environments that are starting to be more common. Of particular focus is the ability to manage origins from the pool via some API.

An eventual goal is to have the pool managed by Raft. This will allow a cluster of redundant weldr servers. This will allow an active/passive setup out of the box. Note: The [raft-rs](https://github.com/hoverbear/raft-rs) crate does not currently support dynamic membership.

## Requirements

   * capnproto
   * A TLS library compatible with rust-tls

### Installing on Ubuntu

```
$ apt-get update && apt-get install gcc libssl-dev pkg-config capnproto
```

### Docker

See [DOCKER.md](./DOCKER.md) for details.

## Running Protype

   * Start the proxy - `RUST_LOG=weldr cargo run --bin weldr`
   * Add a server to the pool - `curl localhost:8687/servers -d '{"url":"http://127.0.0.1:12345"}'`
   * Start test origin server - `cargo run --bin test-server` - start test origin server
   * Send a request - `curl -vvv localhost:8080/`
   * Send a request and get back a large response - `curl -vvv localhost:8080/large`

### Running Tests

`RUST_LOG=test_proxy,weldr cargo test` will execute the tests and provide log level output for both the proxy and the integration tests.

### Benchmarks

See [benchmark/](./benchmark) for details.

## High Level Roadmap

   * Initial [0.1.0](https://github.com/hjr3/weldr/releases/tag/0.1.0<Paste>) release.
   * Currently working on a [0.2.0](https://github.com/hjr3/weldr/milestone/2) release.

## Proposed Management API Design

The management API will allow the addition and removal of origins from the pool. It will also allow for the dynamic configuration of other options, such as the health check.

### Adding A Server

   * Servers must register with the load balancer using an HTTP POST to the management IP.
      * The POST payload contains the health check information.
   * The load balancer will keep that server active in the pool as long as the health succeeds.

```
POST /servers

{
   "url": "http://120.0.0.1"
}
```

Example: `curl -vvv localhost:8687/servers -d '{"url":"http://127.0.0.1"}'`

### Removing A Server

Note: It is more common for a server to fall out of the pool after `n` health checks fail.

```
DELETE /servers/:ip/:port
```

Example: `curl -vvv -X DELETE localhost:8687/servers/127.0.0.1/12345`

### Stats

_Work in progress._

```
GET /stats
```

```
{
   "client": {
      "success": 34534,
      "failed": 33,
   },
   "server": {
      "success": 33770,
      "failed": 15,
   }
}
```

#### Detailed Stats

_Work in progress._

```
GET /stats/detail
```

```
[{
   "id": "...",
   "ip": "127.0.0.1",
   "port": "8080",
   "success": 33770,
   "failed": 15,
},{
   ...
}]
```

## Acknowledgements

   * @hoverbear - for talking through some of the design with me early on
   * @Geal - for talking through some of the design with me early on and sharing code samples
   * @yanns - for setting up the integration tests and implementation help

## License

Licensed under either of
 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
