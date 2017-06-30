#!/bin/bash

set -e

apt-get update
apt-get install build-essential libssl-dev git -y
cd /root
git clone https://github.com/wg/wrk.git
cd /root/wrk
make
cp wrk /usr/local/bin
git clone https://github.com/hjr3/weldr.git
