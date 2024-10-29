use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use anyhow::Result;

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
        let timestamp = chrono::Utc::now().format("%H:%M:%S").to_string();
        let mut message: Vec<u8> = Vec::new();
        message.extend(format!("[{}]", timestamp).as_bytes());
        message.extend(&buffer[..bytes]);
        socket.write_all(&message).await?;
    }
}


