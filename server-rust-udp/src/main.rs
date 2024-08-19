use std::env;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use dashmap::DashMap;
use bytes::BytesMut;
use tokio::spawn;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

type Clients = Arc<DashMap<u64, mpsc::UnboundedSender<BytesMut>>>;

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

async fn handle_connection(
    socket: Arc<UdpSocket>,
    clients: Clients,
    client_id: u64,
    sent_counter: Arc<AtomicUsize>, // Contador compartilhado para contabilizar as mensagens enviadas
    client_addr: std::net::SocketAddr,
) {
    let (tx, mut rx) = mpsc::unbounded_channel::<BytesMut>();
    clients.insert(client_id, tx.clone());

    let sent_counter_clone = Arc::clone(&sent_counter);
    
    // Tarefa para enviar mensagens ao cliente via UDP
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = socket.send_to(&msg, &client_addr).await {
                eprintln!("Erro ao enviar mensagem para o cliente {}: {:?}", client_id, e);
                break;
            }
            
            sent_counter_clone.fetch_add(1, Ordering::Relaxed); 
        }
    });
}

async fn receive_messages(
    socket: Arc<UdpSocket>,
    clients: Clients,
    sent_counter: Arc<AtomicUsize>
) {
    let mut buffer = vec![0; 8192];

    loop {
        let (n, client_addr) = match socket.recv_from(&mut buffer).await {
            Ok((n, addr)) => (n, addr),
            Err(e) => {
                eprintln!("Erro ao receber mensagem: {:?}", e);
                continue;
            }
        };

        if n == 0 {
            continue; // Ignore mensagens vazias
        }

        let received_data = BytesMut::from(&buffer[..n]);

        let client_id = calculate_hash(&client_addr); // Usar um identificador baseado no endereço
        if !clients.contains_key(&client_id) {
            handle_connection(socket.clone(), clients.clone(), client_id, sent_counter.clone(), client_addr).await;
            //println!("Novo cliente conectado: {}", client_addr);
        }

        let clients_clone = clients.clone();
        let sent_counter_inner = Arc::clone(&sent_counter);

        tokio::spawn(async move {
            clients_clone.iter().for_each(|client| {
                let client_sender = client.value().clone();
                let data_clone = received_data.clone(); 

                let sent_counter_clone = Arc::clone(&sent_counter_inner);

                tokio::spawn(async move {
                    if client_sender.send(data_clone).is_ok() {
                        sent_counter_clone.fetch_add(1, Ordering::Relaxed); 
                    }
                });
            });
        });
    }
}

async fn send_ready_message(clients: Clients, sent_counter: Arc<AtomicUsize>) {
    let ready_msg = BytesMut::from("ready\n");

    clients.iter().for_each(|client| {
        let client_sender = client.value().clone();
        let ready_msg_clone = ready_msg.clone();
        let sent_counter_inner = Arc::clone(&sent_counter);

        tokio::spawn(async move {
            if client_sender.send(ready_msg_clone).is_ok() {
                sent_counter_inner.fetch_add(1, Ordering::Relaxed);
            }
        });
    });
}

#[tokio::main]
async fn main() {
    let port: u16 = env::var("PORT").unwrap_or_else(|_| "5001".to_string()).parse().unwrap();
    let clients_to_wait_for: usize = env::var("CLIENTS_COUNT").unwrap_or_else(|_| "32".to_string()).parse().unwrap();

    let clients: Clients = Arc::new(DashMap::new());
    let sent_counter = Arc::new(AtomicUsize::new(0)); 

    let socket = Arc::new(UdpSocket::bind(format!("0.0.0.0:{}", port)).await.unwrap());

    println!("Aguardando {} clientes na porta {}...", clients_to_wait_for, port);

    let counter_clone = sent_counter.clone();
    tokio::spawn(async move {
        loop {
            let messages_sent = counter_clone.swap(0, Ordering::Relaxed);
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            println!("Mensagens enviadas por segundo: {}", messages_sent);
        }
    });

    tokio::spawn(receive_messages(socket.clone(), clients.clone(), sent_counter.clone()));

    // Este loop mantém o servidor ativo indefinidamente
    loop {
        if clients.len() == clients_to_wait_for {
            send_ready_message(clients.clone(), sent_counter.clone()).await;
        }

        // Aguarda um breve intervalo para manter o servidor ativo e funcional
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}
