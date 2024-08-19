FROM rust:1.70 as builder

WORKDIR /app

COPY . .

RUN cargo build --release

WORKDIR /app

CMD ["./target/release/client-rust"]
