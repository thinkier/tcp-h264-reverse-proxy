FROM rust:buster as builder

WORKDIR /env

RUN echo "fn main() {}" > dummy.rs
COPY Cargo.lock .
COPY Cargo.toml .

RUN sed -i 's/src\/main.rs/dummy.rs/' Cargo.toml
RUN cargo build --release; rm dummy.rs
RUN sed -i 's/dummy.rs/src\/main.rs/' Cargo.toml

COPY . .
RUN cargo build --release

FROM debian:buster
WORKDIR /env
RUN apt-get update; apt-get install -y ffmpeg
COPY --from=builder /env/target/release/tcp-h264-reverse-proxy tcp-h264-reverse-proxy
RUN chmod 500 tcp-h264-reverse-proxy

CMD ./tcp-h264-reverse-proxy
