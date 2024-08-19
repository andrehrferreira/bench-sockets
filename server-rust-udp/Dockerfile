FROM rust:1.70 as builder

WORKDIR /app

COPY . .

RUN cargo build --release

WORKDIR /app

EXPOSE 5001

CMD ["./target/release/server-rust-udp"]
