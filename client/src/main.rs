use std::net::SocketAddr;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::time::Duration;
use tokio::{net::{UdpSocket, TcpStream}, io::{AsyncReadExt, AsyncWriteExt}, time::sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use futures_util::{StreamExt, SinkExt};
use futures_util::stream::SplitSink;
use tokio::sync::Mutex; // Usar o Mutex do Tokio

const CLIENTS_TO_WAIT_FOR: usize = 100;
const DELAY: u64 = 64;
const WAIT_TIME_BETWEEN_TESTS: u64 = 5000;
const MESSAGES_TO_SEND: [&str; 20] = [
    "Hello World!", "Hello World! 1", "Hello World! 2", "Hello World! 3",
    "Hello World! 4", "Hello World! 5", "Hello World! 6", "Hello World! 7",
    "Hello World! 8", "Hello World! 9", "What is the meaning of life?",
    "where is the bathroom?", "zoo", "kangaroo", "erlang", "elixir", "bun",
    "mochi", "typescript", "javascript"
];

#[tokio::main]
async fn main() {
    let servers = vec![
        Server { name: "Rust WebSocket".to_string(), url: "ws://127.0.0.1:3001".to_string(), protocol: Protocol::WebSocket },
        Server { name: "Rust TCP".to_string(), url: "127.0.0.1:4001".to_string(), protocol: Protocol::Tcp },
        Server { name: "Rust UDP".to_string(), url: "127.0.0.1:5001".to_string(), protocol: Protocol::Udp },
    ];

    for server in servers {
        test_server(server).await;
        sleep(Duration::from_millis(WAIT_TIME_BETWEEN_TESTS)).await;
    }
}

async fn test_server(server: Server) {
    println!("Connecting to {} at {}", server.name, server.url);

    let received = Arc::new(AtomicUsize::new(0));
    let clients = Arc::new(Mutex::new(vec![]));

    // Clonar o `server` para a tarefa de envio contínuo de mensagens
    let server_for_spawn = server.clone();

    // Conectar clientes e configurar recebimento de mensagens
    for i in 0..CLIENTS_TO_WAIT_FOR {
        let received = Arc::clone(&received);
        let client = create_client(&server, i, received.clone()).await;
        clients.lock().await.push(client);
    }

    // Envio contínuo de mensagens
    let clients_clone = Arc::clone(&clients);
    tokio::spawn(async move {
        loop {
            send_messages_continuously(&server_for_spawn, clients_clone.clone()).await;
            sleep(Duration::from_millis(DELAY)).await;
        }
    });

    // Exibir métricas
    for _ in 0..5 {
        sleep(Duration::from_secs(1)).await;
        let last = received.swap(0, Ordering::Relaxed);
        println!("{}: {} messages per second", server.name, last); // Aqui o `server` original é usado
    }

    // Fechar conexões
    let mut clients_locked = clients.lock().await;
    for client in clients_locked.iter_mut() {
        close_client(client).await;
    }
}


async fn create_client(server: &Server, _client_id: usize, received: Arc<AtomicUsize>) -> Client {
    match server.protocol {
        Protocol::WebSocket => {
            let url = server.url.clone();
            let (ws_stream, _) = connect_async(&url).await.unwrap();
            let (write, mut read) = ws_stream.split();
            let write = Arc::new(Mutex::new(write));
            tokio::spawn(async move {
                while let Some(Ok(msg)) = read.next().await {
                    if let Message::Text(_) = msg {
                        received.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
            Client::WebSocket(write)
        },
        Protocol::Tcp => {
            let addr: SocketAddr = server.url.parse().unwrap();
            let stream = TcpStream::connect(addr).await.unwrap();
            let (reader, writer) = tokio::io::split(stream);
            let writer = Arc::new(Mutex::new(writer));
            tokio::spawn(async move {
                let mut buffer = vec![0; 1024];
                let mut reader = reader;
                loop {
                    match reader.read(&mut buffer).await {
                        Ok(n) if n > 0 => {
                            received.fetch_add(1, Ordering::Relaxed);
                        },
                        Ok(_) | Err(_) => break,
                    }
                }
            });
            Client::Tcp(writer)
        },
        Protocol::Udp => {
            let socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await.unwrap());
            let server_addr: SocketAddr = server.url.parse().unwrap();
            let socket_clone = Arc::clone(&socket); // Usar Arc para compartilhamento
            let mut buf = vec![0; 1024];
            tokio::spawn(async move {
                loop {
                    match socket_clone.recv_from(&mut buf).await {
                        Ok((n, _)) if n > 0 => {
                            received.fetch_add(1, Ordering::Relaxed);
                        },
                        Ok(_) => break,
                        Err(_) => break,
                    }
                }
            });
            Client::Udp(socket, server_addr)
        }
    }
}

async fn send_messages_continuously(server: &Server, clients: Arc<Mutex<Vec<Client>>>) {
    let mut clients_locked = clients.lock().await;
    for client in clients_locked.iter_mut() {
        for msg in MESSAGES_TO_SEND.iter() {
            match client {
                Client::WebSocket(writer) => {
                    let mut writer = writer.lock().await;
                    let _ = writer.send(Message::Text(msg.to_string())).await;
                }
                Client::Tcp(writer) => {
                    let mut writer = writer.lock().await;
                    let _ = writer.write_all(msg.as_bytes()).await;
                }
                Client::Udp(socket, addr) => {
                    let _ = socket.send_to(msg.as_bytes(), *addr).await;
                }
            }
        }
    }
}

async fn close_client(client: &mut Client) {
    match client {
        Client::WebSocket(writer) => {
            let mut writer = writer.lock().await;
            let _ = writer.close().await;
        },
        Client::Tcp(_writer) => {} // TCP connection is automaticamente fechada
        Client::Udp(_socket, _addr) => {} // UDP socket is automaticamente fechado
    }
}

enum Client {
    WebSocket(Arc<Mutex<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>),
    Tcp(Arc<Mutex<tokio::io::WriteHalf<TcpStream>>>),
    Udp(Arc<UdpSocket>, SocketAddr),
}

#[derive(Clone)]
struct Server {
    name: String,
    url: String,
    protocol: Protocol,
}

#[derive(Clone)]
enum Protocol {
    WebSocket,
    Tcp,
    Udp,
}
