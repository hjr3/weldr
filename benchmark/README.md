# Benchmarks

## Environment

[Bare metal Type 2 Packet instance 24 Physical Cores @ 2.2 GHz (2 × E5-2650 v4)](https://www.packet.net/bare-metal/)

   * The [Techempower benchmarks](https://www.techempower.com/benchmarks/#section=environment&hw=ph&test=fortune) use a [2.0 GHz processor](http://ark.intel.com/products/53574/Intel-Xeon-Processor-E7-4850-24M-Cache-2_00-GHz-6_40-GTs-Intel-QPI) on the Dell R910

Need to add at least one server to the proxy prior to testing:

```
$ curl -vvv -H "Host: www.example.com" localhost:8687/servers -d '{"url":"http://127.0.0.1:12345"}'
```

Note: For a real test, should be running on its own server.

## Tests

   * techempower plaintext benchmark per https://www.techempower.com/benchmarks/#section=code

### wrk

To run the plaintext benchmark using `wrk`:

   * example command: `wrk --connection 100 --duration 30s --threads 4 http://localhost:8080`
   * techempower plaintext benchmark: `wrk -c 256 -t 32 -d 15 -s ./benchmark/pipeline.lua --latency http://localhost:8080 -- 16`

Note: For a real test, `wrk` should be running on its own server.

### Flamegraphs

Add the following to `Cargo.toml`:

```toml
[profile.release]
debug = true
```
