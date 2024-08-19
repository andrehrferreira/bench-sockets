FROM rust:1.70 as builder

WORKDIR /app

COPY . .

RUN cargo build --release

WORKDIR /app

EXPOSE 4001

CMD ["./target/release/server-rust-tcp"]
