#!/bin/bash

set -e

apt-get update
apt-get install gcc libssl-dev pkg-config capnproto git -y
cd /root
curl https://sh.rustup.rs -sSf | sh -s -- -y
. /root/.cargo/env
git clone https://github.com/hjr3/weldr.git
cd /root/weldr
cargo build --release
