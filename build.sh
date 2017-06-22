#!/usr/bin/env bash

apt update && apt install capnproto -y && cargo build --release