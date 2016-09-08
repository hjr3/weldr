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




### Tokio notes

[13:40:24]  <moosnat>	Service is explained pretty well in the medium post -- basically a composable middleware
[13:40:31]  <moosnat>	I don't really understand the others though
[14:16:35]  <tikue>	moosnat: transport basically encodes how to read and write messages 
[14:16:56]  <tikue>	moosnat: readiness is how the transport tells the event loop that read or write should be invoked on any given tick
[14:17:33]  <tikue>	so service = application layer, transport = the application's underlying protocol, whether that is http or some other custom protocol
[14:17:55]  <moosnat>	tikue: so I'd write the communication logic in the transport
[14:18:02]  <tikue>	moosnat: the nice thing is that, if you're using a common transport like http, you don't have to write it yourself, and you just have to concern yourself with the Service layer
[14:18:04]  <moosnat>	the high-level application in service

[16:43:14]  <~carllerche>	tikue: moosnat: you can stack transports. Pipeline is actually not a transport
[16:43:32]  <~carllerche>	Pipeline is the task implementation. It takes a transport and exposes it as a service.
[16:43:59]  <~carllerche>	Pipeline is a reusable glue layer between transport and service
[16:44:07]  <~carllerche>	For pipeline based protocols
