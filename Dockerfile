# create and run build environment
FROM rust:1.50 as builder
ENV BUILDIR=/usr/src/resweb

RUN cargo install cargo-build-deps
RUN mkdir -p ${BUILDIR}
RUN cd ${BUILDIR} && USER=root cargo init --bin .
WORKDIR ${BUILDIR}
COPY Cargo.toml Cargo.lock ./
RUN cargo build-deps --release

COPY src ${BUILDIR}/src
RUN cargo build --release

# create runtime environment image
FROM alpine:3.13

RUN apk add --no-cache \
        ca-certificates \
        openssl

COPY --from=builder /usr/local/cargo/bin/resweb /usr/local/bin/resweb