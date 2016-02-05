# Alacrity

A tcp load balancer written in Rust.

## Design

Alacrity uses coroutines to take advantage of async-IO and threads to maximize throughput.

HAProxy is really big on making the stats easy. It does this using a webpage though. I wonder if I should consider adding a simple API for things like stats and high level monitoring.

### Clustering

For clustiner, Alacrity must be setup using DNS round-robin.

### High Availability

Alacrity has an active/passive setup that works out of the box.

## HA Proxy

Example HA Proxy config: http://www.haproxy.org/download/1.4/doc/configuration.txt

## License

Licensing this under GPL (not sure version) or a commercial license. Is this compatible with MPL, MIT and Apache licenses?
