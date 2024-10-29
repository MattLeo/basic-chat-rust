use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use anyhow::Result;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    let ip = "127.0.0.1";
    let port = "9090";
    let username = "TheLeo";

    let stream = TcpStream::connect(format!("{}:{}", ip, port)).await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    let (tx, mut rx) = mpsc::channel::<String>(32);

    let write_handler = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            let combined_message: String = format!("{}: {}", username, message);
            if writer.write_all(combined_message.as_bytes()).await.is_err() {
                break;
            }
        }
    });

    let input_handler = tokio::spawn(async move {
        let mut buffer = String::new();
        let mut stdin = BufReader::new(tokio::io::stdin());

        loop {
            buffer.clear();
            if stdin.read_line(&mut buffer).await.is_ok() {
                print!("\x1B[1A\x1B[2K");
                if buffer.trim().eq("/quit") {
                    break;
                }
                if tx.send(buffer.clone()).await.is_err() {
                    break;
                }
            }
        }
    });

    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                println!("Server closed the connection");
                break;
            }
            Ok(_) => {
                print!("{}", line);
            }
            Err(e) => {
                eprintln!("Error reading from server: {}", e);
                break;
            }
        }
    }

    input_handler.abort();
    write_handler.abort();

    Ok(())
}
