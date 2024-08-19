Benchmark Websocket, TCP and UDP in Rust
=========================

Build Websocket Server
-------------

```bash
$ cd server-rust-ws
$ cargo build --release
```

Run
```bash
$ ./target/release/server-rust-ws
```

Docker
```bash
$ cd server-rust-ws
$ docker build -t server-rust-ws .
$ docker run -p 3001:3001 server-rust-ws
```

Build TCP/IP Server
-------------

```bash
$ cd server-rust-tcp
$ cargo build --release
```

Run
```bash
$ ./target/release/server-rust-tcp
```

Docker
```bash
$ cd server-rust-ws
$ docker build -t server-rust-tcp .
$ docker run -p 4001:4001 server-rust-tcp
```

Build Client
-------------

```bash
$ cd client
$ cargo build --release
```

Run
```bash
$ ./target/release/client-rust
```