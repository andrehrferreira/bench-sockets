use std::env;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use dashmap::DashMap;
use bytes::BytesMut;
use tokio::spawn;

type Clients = Arc<DashMap<usize, mpsc::UnboundedSender<BytesMut>>>;

async fn handle_connection(
    raw_stream: TcpStream,
    clients: Clients,
    client_id: usize,
    sent_counter: Arc<AtomicUsize>, // Contador compartilhado para contabilizar as mensagens enviadas
) {
    let (mut reader, mut writer) = raw_stream.into_split();
    
    let (tx, mut rx) = mpsc::unbounded_channel::<BytesMut>();
    clients.insert(client_id, tx.clone());

    let sent_counter_clone = Arc::clone(&sent_counter);
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = writer.write_all(&msg).await {
                break;
            }
            
            sent_counter_clone.fetch_add(1, Ordering::Relaxed); 
        }
    });

    let mut buffer = BytesMut::with_capacity(8192);

    while let Ok(n) = reader.read_buf(&mut buffer).await {
        if n == 0 {
            break; 
        }

        let received_data = buffer.split_to(n);

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

    clients.remove(&client_id);
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
    let port: u16 = env::var("PORT").unwrap_or_else(|_| "4001".to_string()).parse().unwrap();
    let clients_to_wait_for: usize = env::var("CLIENTS_COUNT").unwrap_or_else(|_| "32".to_string()).parse().unwrap();

    let clients: Clients = Arc::new(DashMap::new());
    let sent_counter = Arc::new(AtomicUsize::new(0)); 

    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();

    println!("Waiting for {} clients to connect...", clients_to_wait_for);

    let mut client_id_counter = 0;

    let counter_clone = sent_counter.clone();
    tokio::spawn(async move {
        loop {
            let messages_sent = counter_clone.swap(0, Ordering::Relaxed);
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    });

    while let Ok((stream, _)) = listener.accept().await {
        let clients = Arc::clone(&clients);
        let client_id = client_id_counter;
        client_id_counter += 1;

        let sent_counter_clone = Arc::clone(&sent_counter);

        tokio::spawn(async move {
            handle_connection(stream, clients.clone(), client_id, sent_counter_clone.clone()).await;
        
            if clients.len() == clients_to_wait_for {
                send_ready_message(clients.clone(), sent_counter_clone).await;
            }
        });
    }
}
