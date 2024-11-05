use anyhow::Result;
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use serde_json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

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

    let stream = TcpStream::connect(format!("{}:{}", ip, port)).await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    let (tx, mut rx) = mpsc::channel::<String>(32);

    let write_handler = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if writer.write_all(message.as_bytes()).await.is_err() {
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
                //println!("Raw data received from server: {:?}", line);
                if line.starts_with("JSON:") {
                    if let Ok(json_data) = serde_json::from_str::<MessageData>(&line[5..]) {
                        let local_time =
                            json_data.timestamp.with_timezone(&Local).format("%H:%M:%S");
                        print!("[{}]{}: {}",
                            local_time,
                            json_data.username,
                            String::from_utf8_lossy(&json_data.message)
                        );
                    }
                } else if line.starts_with("PM:") {
                    if let Ok(json_data) = serde_json::from_str::<MessageData>(&line[3..]) {
                        let local_time =
                            json_data.timestamp.with_timezone(&Local).format("%H:%M:%S");
                        print!("[{}] Whisper from {}: {}",
                            local_time,
                            json_data.username,
                            String::from_utf8_lossy(&json_data.message)
                        );
                    }
                } else {
                    println!("{}", line.trim());
                }
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
