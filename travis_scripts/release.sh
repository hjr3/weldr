#!/usr/bin/env bash

function main {
    if [ ! -z $1 ]; then
      mkdir -p opt;
      cp target/x86_64-unknown-linux-musl/release/weldr opt/;
      # Use the same docker image as much as possible
      docker run -v $PWD:/volume -w /volume -t clux/muslrust /volume/build.sh;
      docker run -v $(pwd):/src/ cdrx/fpm-centos:7 -s dir -t deb -v $1 -n weldr -C /src  opt/weldr;
      docker run -v $(pwd):/src/ cdrx/fpm-centos:7 -s dir -t rpm -v $1 -n weldr -C /src  opt/weldr;

      # Publish to docker hub
      docker login -u=$DOCKER_USERNAME -p=$DOCKER_PASSWORD;
      docker build -t weldr/weldr:x86_64 .
      docker push weldr/weldr:x86_64;
    fi
}

# Syntax: ./release.sh <travis_tag>
# Usage:
#   export DOCKER_USERNAME=myuser
#   export DOCKER_PASSWORD=mypass
#   ./release.sh 0.1.0
main $*

