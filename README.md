# Alacrity

A tcp proxy written in Rust.

## Design

The goal is to build a proxy that works well in the dynamic VM/container environments that are starting to be more common.

   * Servers must register with the proxy using an HTTP POST to the management IP.
      * The POST payload contains the health check information.
   * The proxy will keep that server active in the pool as long as the health succeeds.
   * The pool is managed by Raft, allowing a cluster of redundant proxy servers. This should allow an active/passive setup out of the box.
      * Note: The [raft-rs](https://github.com/Hoverbear/raft-rs) crate does not currently support dynamic membership.
   * Async IO is done using tokio-core (which is built on top of mio).

Credit to Hoverbear who talked through some of the design with me.

## Running Protype

   * `cargo run --bin test-server`
   * `cargo run --bin alacrity`
   * `echo "hi" | nc localhost 8080`

## High Level Roadmap

   * [x] Proxy prototype using tokio-core
   * [ ] Create Server pool
   * [ ] Management API
   * [ ] Support health checks
   * [ ] Server pool managed by raft

## Management API Design

### Adding A Server

```
POST /servers

{
   "ip": "120.0.0.1",
   "port": "8080",
   "check": {
      "type": "tcp"
   }
}
```

### Removing A Server

Note: It is more common for a server to fall out of the pool after `n` health checks fail.

```
DELETE /servers/:id
```

### Stats

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

## License

Dunno yet!
