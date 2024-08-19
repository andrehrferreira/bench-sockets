use futures_util::{stream::StreamExt, SinkExt};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{self, Duration, timeout};
use tokio::spawn;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

#[derive(Clone)]
enum ServerType {
    WebSocket,
    TCP,
}

#[derive(Clone)]
struct Server {
    name: &'static str,
    url: &'static str,
    server_type: ServerType,
}

const SERVERS: &[Server] = &[
    Server { name: "Rust Websocket", url: "ws://127.0.0.1:3001", server_type: ServerType::WebSocket },
    Server { name: "Rust TCP", url: "127.0.0.1:4001", server_type: ServerType::TCP },
];

const CLIENTS_TO_WAIT_FOR: usize = 32;
const WAIT_TIME_BETWEEN_TESTS: u64 = 5000;
const SEND_RATE_MS: u64 = 64;
const MESSAGES_TO_SEND: &[&str] = &[
    "Hello World!",
    "What is the meaning of life?",
    "zoo",
    "kangaroo",
    "erlang",
    "elixir",
    "bun",
    "mochi",
    "typescript",
    "javascript"
];

#[derive(Debug)]
struct Result {
    name: String,
    average: f64,
    lost_packets: usize,
    percentage: f64,
}

#[tokio::main]
async fn main() {
    let mut results = Vec::new();

    for server in SERVERS {
        let result = test_server(server.clone()).await;
        results.push(result);
        time::sleep(Duration::from_millis(WAIT_TIME_BETWEEN_TESTS)).await;
    }

    let overall_average: f64 = results.iter().map(|r| r.average).sum::<f64>() / results.len() as f64;

    for result in &mut results {
        result.percentage = ((result.average - overall_average) / overall_average) * 100.0;
    }

    results.sort_by(|a, b| b.average.partial_cmp(&a.average).unwrap());

    println!("{:<20} {:<20} {:<20} {:<20}", "Server", "Avg Messages/sec", "Lost Packets", "% Difference");
    for result in results {
        println!("{:<20} {:<20.2} {:<20} {:<20.2}%", result.name, result.average, result.lost_packets, result.percentage);
    }
}

async fn test_server(server: Server) -> Result {
    println!("Connecting to {} at {}", server.name, server.url);

    let received = Arc::new(AtomicUsize::new(0));
    let lost_packets = Arc::new(AtomicUsize::new(0));
    let mut connections = Vec::new();

    for _ in 0..CLIENTS_TO_WAIT_FOR {
        let server_type = server.server_type.clone();
        let url = server.url.to_string();
        let received_clone = Arc::clone(&received);
        let lost_packets_clone = Arc::clone(&lost_packets);

        match server_type {
            ServerType::WebSocket => {
                // Conexão WebSocket
                match connect_async(&url).await {
                    Ok((ws_stream, _)) => {
                        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

                        let receive_task = spawn(async move {
                            while let Some(msg) = ws_receiver.next().await {
                                if let Ok(Message::Text(_)) = msg {
                                    received_clone.fetch_add(1, Ordering::Relaxed);
                                } else {
                                    break;
                                }
                            }
                        });

                        let send_task = spawn(async move {
                            loop {
                                for &msg in MESSAGES_TO_SEND.iter() {
                                    if ws_sender.send(Message::Text(msg.to_string())).await.is_err() {
                                        lost_packets_clone.fetch_add(1, Ordering::Relaxed);
                                        break;
                                    }
                                }
                                time::sleep(Duration::from_millis(SEND_RATE_MS)).await;
                            }
                        });

                        connections.push(receive_task);
                        connections.push(send_task);
                    }
                    Err(e) => {
                        println!("Failed to connect to WebSocket server {}: {:?}", server.url, e);
                    }
                }
            }
            ServerType::TCP => {
                // Conexão TCP
                match TcpStream::connect(&url).await {
                    Ok(stream) => {
                        let (mut reader, mut writer) = stream.into_split();

                        let receive_task = spawn(async move {
                            let mut buffer = vec![0; 8192];
                            loop {
                                match reader.read(&mut buffer).await {
                                    Ok(0) => break, // Conexão fechada
                                    Ok(_) => {
                                        received_clone.fetch_add(1, Ordering::Relaxed);
                                    }
                                    Err(_) => break, // Erro ao ler do socket
                                }
                            }
                        });

                        let send_task = spawn(async move {
                            loop {
                                for &msg in MESSAGES_TO_SEND.iter() {
                                    if writer.write_all(msg.as_bytes()).await.is_err() {
                                        lost_packets_clone.fetch_add(1, Ordering::Relaxed);
                                        break;
                                    }
                                }
                                time::sleep(Duration::from_millis(SEND_RATE_MS)).await;
                            }
                        });

                        connections.push(receive_task);
                        connections.push(send_task);
                    }
                    Err(e) => {
                        println!("Failed to connect to TCP server {}: {:?}", server.url, e);
                    }
                }
            }
        }
    }

    // Limitar o tempo de execução de cada teste a 5 segundos
    let _ = timeout(Duration::from_secs(5), async {
        let mut runs = Vec::new();
        for _ in 0..5 {
            time::sleep(Duration::from_secs(1)).await;
            let count = received.swap(0, Ordering::Relaxed);
            runs.push(count);

            if count == 0 {
                lost_packets.fetch_add(1, Ordering::Relaxed);
            }
            println!("{}: {} messages per second", server.name, count);
        }

        let sum: usize = runs.iter().sum();
        let average = sum as f64 / runs.len() as f64;
        let lost_packets = lost_packets.load(Ordering::Relaxed);

        // Esperar todas as conexões terminarem
        for conn in connections {
            let _ = conn.await;
        }

        Result {
            name: server.name.to_string(),
            average,
            lost_packets,
            percentage: 0.0,
        }
    }).await;

    // Finalizando as conexões caso estejam ativas
    Result {
        name: server.name.to_string(),
        average: received.load(Ordering::Relaxed) as f64 / 5.0,
        lost_packets: lost_packets.load(Ordering::Relaxed),
        percentage: 0.0,
    }
}
