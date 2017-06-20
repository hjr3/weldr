# Docker

Resources:

- [Docker & Rust: Statically Linking Binaries for Secure Execution in Untrusted Containers](http://betacs.pro/blog/2016/07/07/docker-and-rust/)
- [Docker environment for building musl based static rust binaries](https://github.com/clux/muslrust)

## Build a statically linked binary

### On linux:
```
rustup target add x86_64-unknown-linux-musl
cargo build --target x86_64-unknown-linux-musl --release
```

### Other (mac):
```
docker run -v $PWD:/volume -w /volume -t clux/muslrust /volume/docker.sh
```

The executable can be found under: `./target/x86_64-unknown-linux-musl/release/weldr`

## Create the docker container
```
docker build -t weldr .
```

## Test the docker container
```
docker run -p 8080:8080 -p 8687:8687 \
-e "RUST_BACKTRACE=1" -e "RUST_LOG=weldr" \
--name weldr --rm \
weldr \
/weldr --ip 0.0.0.0:8080 --admin-ip 0.0.0.0:8687
```
Explanations:
- `-p 8080:8080 -p 8687:8687`: bind the ports 8080 and 8687 to localhost
- `-e "RUST_BACKTRACE=1" -e "RUST_LOG=weldr"`: set the environment variables `RUST_BACKTRACE` and `RUST_LOG`
- `--name weldr`: name the running container
- `weldr`: container image
- `/weldr 0.0.0.0:8080 127.0.0.1:12345 0.0.0.0:8687`: start weldr listening on all IPs.

The proxy is accessible on [localhost:8080](http://localhost:8080) and the admin interface is accessible on [localhost:8687](http://localhost:8687)
or with docker-machine:
```
m=`docker-machine active`
ip=`docker-machine ip $m`
url=http://$ip:8687
echo $url
curl $url/servers
```

Stops the test container with:
```
docker rm -vf weldr
```
