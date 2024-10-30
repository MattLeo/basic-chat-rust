use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};


#[derive(Serialize, Deserialize, Debug)]
struct MessageData {
   timestamp: DateTime<Utc>,
   username: String,
   message: Vec<u8>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let ip = "127.0.0.1";
    let port = "9090";
    let listener = TcpListener::bind(format!("{}:{}", ip, port)).await.expect("Failed to bind to address");
    println!("Server listening on {}:{}", ip, port);

    loop{
        let (socket, addr) = listener.accept().await?;
        println!("New connection from: {}", addr);
        tokio::spawn(async move {
            if let Err(e) = manage_connection(socket).await {
                eprint!("Connection error received: {}", e);
            }
        });
    }
}

async fn manage_connection(mut socket: TcpStream) -> Result<()> {
    let mut buffer = [0u8;1024];

    loop {
        let bytes = socket.read(&mut buffer).await?;
        if bytes == 0 {
            println!("Client has disconnected");
            return Ok(());
        }
        let timestamp = chrono::Utc::now();
        let mut message: Vec<u8> = Vec::new();
        let username = "TheLeo".to_string();
        message.extend_from_slice(&buffer[..bytes]);
        let message_data = MessageData {
            timestamp,
            username,
            message,
        };
        let json_data = serde_json::to_string(&message_data)? + "\n";
        socket.write_all(json_data.as_bytes()).await?;
    }
}

