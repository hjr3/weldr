# Benchmarks

## Environment

Bare metal Rackspace instance Intel(R) Xeon(R) CPU E5-2680 v2 @ 2.80GHz 20 physical cores 40 hyperthreaded CPU

   * https://www.rackspace.com/en-us/cloud/servers/onmetal/specs see Compute specs

Need to add at least one server to the proxy prior to testing:

```
$ curl -vvv -H "Host: www.example.com" localhost:8687/servers -d '{"url":"http://127.0.0.1:12345"}'
```

## Tests

   * techempower plaintext benchmark per https://www.techempower.com/benchmarks/#section=code

### wrk

   * example command: `wrk --connection 100 --duration 30s --threads 4 http://localhost:8080`
   * techempower plaintext benchmark: `wrk -c 256 -t 32 -d 15 -s ./benchmark/pipeline.lua --latency http://localhost:8080 -- 16`

### Flamegraphs

Add the following to `Cargo.toml`:

```toml
[profile.release]
debug = true
```
