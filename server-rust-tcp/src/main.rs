use std::env;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener};
use tokio::sync::mpsc;
use dashmap::DashMap;

type Clients = Arc<DashMap<usize, mpsc::UnboundedSender<Vec<u8>>>>;

async fn handle_connection(
    raw_stream: tokio::net::TcpStream,
    clients: Clients,
    client_id: usize,
    clients_to_wait_for: usize,
) {
    let (mut reader, mut writer) = raw_stream.into_split();
    
    // Criação de canal para enviar mensagens ao cliente
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    clients.insert(client_id, tx.clone());

    let current_clients = clients.len();

    // Verificar se todos os clientes estão conectados
    if current_clients >= clients_to_wait_for {
        send_ready_message(clients.clone()).await;
    }

    // Task para enviar mensagens ao cliente
    let write_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if writer.write_all(&msg).await.is_err() {
                break; // Se ocorrer um erro, parar o envio
            }
        }
    });

    // Buffer de leitura
    let mut buffer = vec![0; 8192];

    // Loop para receber mensagens do cliente e transmiti-las
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => break, // Conexão fechada
            Ok(n) => {
                let received_data = buffer[..n].to_vec();
                
                // Enviar a mensagem recebida para todos os outros clientes
                clients.iter().for_each(|client| {
                    if *client.key() != client_id {
                        let _ = client.value().send(received_data.clone());
                    }
                });
            }
            Err(_) => break, // Em caso de erro, encerrar a conexão
        }
    }

    // Remover o cliente da lista ao desconectar
    clients.remove(&client_id);
    write_task.await.unwrap();
}

async fn send_ready_message(clients: Clients) {
    let ready_msg = b"ready\n".to_vec();
    for client_id_key in clients.iter().map(|client| *client.key()) {
        if let Some(client_sender) = clients.get(&client_id_key) {
            let _ = client_sender.send(ready_msg.clone());
        }
    }
}

#[tokio::main]
async fn main() {
    let port: u16 = env::var("PORT").unwrap_or_else(|_| "4001".to_string()).parse().unwrap();
    let clients_to_wait_for: usize = env::var("CLIENTS_COUNT").unwrap_or_else(|_| "32".to_string()).parse().unwrap();

    let clients: Clients = Arc::new(DashMap::new());
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();

    println!("Esperando por {} clientes na porta {}...", clients_to_wait_for, port);

    let mut client_id_counter = 0;

    while let Ok((stream, _)) = listener.accept().await {
        let clients_clone = Arc::clone(&clients);
        let client_id = client_id_counter;
        client_id_counter += 1;

        tokio::spawn(async move {
            handle_connection(stream, clients_clone, client_id, clients_to_wait_for).await;
        });
    }
}
