FROM scratch
MAINTAINER Herman J. Radtke III <herman@hermanradtke.com>
MAINTAINER Yann Simon <yann.simon.fr@gmail.com>

COPY ./target/x86_64-unknown-linux-musl/release/alacrity /alacrity
CMD ["/alacrity"]