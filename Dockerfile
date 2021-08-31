FROM ubuntu:18.04 as builder

RUN apt-get update

# https://stackoverflow.com/questions/44331836/apt-get-install-tzdata-noninteractive
RUN DEBIAN_FRONTEND=noninteractive apt-get install -y tzdata

RUN apt-get -y install curl gcc llvm openssl libssl-dev pkg-config
RUN useradd -m rust

USER rust
WORKDIR /home/rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs > rustup-init.sh
RUN chmod +x rustup-init.sh
RUN ./rustup-init.sh -y
RUN echo '$HOME/.cargo/env' >> ~/.profile

RUN umask 002; mkdir /home/rust/resweb
RUN chmod u+x .
WORKDIR /home/rust/resweb

# create build layer that only builds dependencies
RUN ~/.cargo/bin/cargo init .
COPY Cargo.* .
RUN ~/.cargo/bin/cargo build --release

COPY --chown=rust src src/
RUN touch src/main.rs 

RUN ~/.cargo/bin/cargo build --release


# create runtime environment image
FROM ubuntu:18.04

RUN apt-get update
RUN apt-get install -y openssl

ENV TARGET_DIR=/usr/local/bin
RUN mkdir -p $TARGET_DIR
COPY --from=builder /home/rust/resweb/target/*/resweb $TARGET_DIR

ENTRYPOINT ["/usr/local/bin/resweb"]