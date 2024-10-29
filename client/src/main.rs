use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use anyhow::Result;
use std::io;

#[tokio::main]
async fn main() -> Result<()> {
    let ip = "120.0.0.1";
    let port = "9090";

    let mut socket = TcpStream::connect(format!("{}:{}", ip, port)).await?;
    
}