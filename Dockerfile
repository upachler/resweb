FROM ubuntu

RUN apt-get update
RUN apt-get -y install curl
RUN apt-get -y install gcc llvm
RUN useradd -m rust
USER rust
WORKDIR /home/rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs > rustup-init.sh
RUN chmod +x rustup-init.sh
RUN ./rustup-init.sh -y
RUN echo '$HOME/.cargo/env' >> ~/.profile

#RUN ls ~/.cargo/bin && echo $PATH && false
WORKDIR $HOME/resweb
RUN mkdir resweb
COPY . .

USER root
RUN apt-get -y install openssl libssl-dev

# https://stackoverflow.com/questions/44331836/apt-get-install-tzdata-noninteractive
RUN DEBIAN_FRONTEND=noninteractive apt-get install -y tzdata

RUN apt-get -y install pkg-config

USER rust
RUN ~/.cargo/bin/cargo build