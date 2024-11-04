use tokio::io::{self, AsyncReadExt, AsyncWriteExt, BufReader, AsyncBufReadExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use sled::Db;
use std::sync::Arc;
use uuid::Uuid;
use bcrypt::{hash, verify, DEFAULT_COST};


#[derive(Serialize, Deserialize, Debug)]
struct MessageData {
   timestamp: DateTime<Utc>,
   username: String,
   message: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
struct UserAccount {
    username: String,
    uuid: String,
    pass_hash: String,
    server_role: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let ip = "127.0.0.1";
    let port = "9090";
    println!("Initializing internal DBs..");
    let user_db = open_db("user_db")?;
    let channel_db = open_db("channel_db")?;
    let listener = TcpListener::bind(format!("{}:{}", ip, port)).await.expect("Failed to bind to address");
    println!("Server listening on {}:{}", ip, port);


    let user_db_clone = Arc::clone(&user_db);
    let channel_db_clone = Arc::clone(&channel_db);
    tokio::spawn(async move {
        server_command_handler(user_db_clone, channel_db_clone).await;
    });

    loop{
        let (socket, addr) = listener.accept().await?;
        println!("New connection from: {}", addr);
        let user_db = Arc::clone(&user_db);
        let channel_db = Arc::clone(&channel_db);
        tokio::spawn(async move {
            if let Err(e) = manage_connection(socket, user_db, channel_db).await {
                eprint!("Connection error received: {}", e);
            }
        });
    }
}

async fn manage_connection(socket: TcpStream, user_db: Arc<Db>, channel_db: Arc<Db>) -> Result<()> {
    let mut buffer = [0u8;1024];
    let shared_socket = Arc::new(Mutex::new(socket));
    
    let user = auth_user(user_db, Arc::clone(&shared_socket)).await?;
    let mut current_channel = "general".to_string();
    {
        let mut locked_socket = shared_socket.lock().await;
        locked_socket.write_all(format!("Joining channel: {}...\n", current_channel).as_bytes()).await?;
    };
    let _ = display_channel_history(current_channel.clone(), Arc::clone(&channel_db), Arc::clone(&shared_socket)).await;

    loop {
        let bytes = {
            let mut locked_socket = shared_socket.lock().await;
            locked_socket.read(&mut buffer).await?
        };
        if bytes == 0 {
            println!("Client has disconnected");
            return Ok(());
        }

        let input = String::from_utf8_lossy(&buffer[..bytes]);
        if input.starts_with('/') {
            let mut parts = input.trim().splitn(2, ' ');
            let command = parts.next().unwrap();
            let argument = parts.next();

            match command {
                "/join" => {
                    if let Some(channel_name) = argument {
                        current_channel = channel_name.to_string();
                        let response = format!("Joined channel: {}\n", current_channel);
                        let mut locked_socket = shared_socket.lock().await;
                        locked_socket.write_all(response.as_bytes()).await?;
                    } else {
                        let error = "Usage: /join <channel>\n";
                        let mut locked_socket = shared_socket.lock().await;
                        locked_socket.write_all(error.as_bytes()).await?;
                    }
                }
                "/leave" => {
                    if current_channel == "general" {
                        let response = format!("Cannot leave the general channel. Use /join to select new channel.");
                        let mut locked_socket = shared_socket.lock().await;
                        locked_socket.write_all(response.as_bytes()).await?;
                    } else {
                        let response = format!("You have left {}. Joining general channel..", current_channel);
                        current_channel = "general".to_string();
                        let mut locked_socket = shared_socket.lock().await;
                        locked_socket.write_all(response.as_bytes()).await?;
                    }
                }
                _=> {
                    let error = format!("Unknown command: {}", command);
                    let mut locked_socket = shared_socket.lock().await;
                    locked_socket.write_all(error.as_bytes()).await?;
                }
            }
        } else {
            let timestamp = chrono::Utc::now();
            let message_data = MessageData {
                timestamp,
                username: user.username.clone(),
                message: input.as_bytes().to_vec(),
            };
            let serialized_message = serde_json::to_vec(&message_data)?;
            let message_key = format!("{}:{}:{}", current_channel, timestamp, user.uuid.clone());
            channel_db.insert(message_key, serialized_message)?;
            let _ = channel_db.flush()?;

            let json_data = format!("{}{}{}", "JSON:" ,serde_json::to_string(&message_data)?, "\n");
            let mut locked_socket = shared_socket.lock().await;
            locked_socket.write_all(json_data.as_bytes()).await?;
        }
    }
}


fn open_db(db_name: &str) -> Result<Arc<Db>> {
    let db = sled::open(db_name)?;
    Ok(Arc::new(db))
}

async fn auth_user(user_db: Arc<Db>, shared_socket: Arc<Mutex<TcpStream>>) -> Result<UserAccount> {
    let mut buffer = [0u8; 1024];
    {
        let mut locked_socket = shared_socket.lock().await;
        locked_socket.write_all(b"Please enter username:\n").await?;
        locked_socket.flush().await?;
    }
    let username = {
        let bytes = {
            let mut locked_socket = shared_socket.lock().await;
            locked_socket.read(&mut buffer).await?
        };
        String::from_utf8_lossy(&buffer[..bytes]).trim().to_string()
    };
    let user_data = match user_db.get(username.as_bytes())? {
        Some(data) => data,
        None => {
            return register_user(username, shared_socket, user_db).await;
        }
    };
    let user: UserAccount = serde_json::from_slice(&user_data)
        .map_err(|e| anyhow::anyhow!("Failed to deserialize user data: {}", e))?;
    {
        let mut locked_socket = shared_socket.lock().await;
        locked_socket.write_all(b"Please enter password:\n").await?;
    };
    let password = {
        let bytes = {
            let mut locked_socket = shared_socket.lock().await;
            locked_socket.read(&mut buffer).await?
        };
        String::from_utf8_lossy(&buffer[..bytes]).trim().to_string()
    };
    if verify(&password, &user.pass_hash)? {
        let mut locked_socket = shared_socket.lock().await;
        locked_socket.write_all(b"Authentication successful\n").await?;
        Ok(user)
    } else {
        let mut locked_socket = shared_socket.lock().await;
        locked_socket.write_all(b"Invalid Password\n").await?;
        anyhow::bail!("Authentication failed: invalid password");
    }
}

async fn register_user(username: String, shared_socket: Arc<Mutex<TcpStream>>, user_db: Arc<Db>) -> Result<UserAccount> {
    let mut buffer = [0u8; 1024];
    {
        let mut locked_socket = shared_socket.lock().await;
        locked_socket.write_all(b"Username not found. Would you like to register this username? (Y/N)\n").await?;
    };
    let response = {
        let bytes =  {
            let mut locked_socket = shared_socket.lock().await;
            locked_socket.read(&mut buffer).await?
        };
        String::from_utf8_lossy(&buffer[..bytes]).trim().to_string()
    };
    match response.as_str() {
        "Y" | "y" => {
            {
                let mut locked_socket = shared_socket.lock().await;
                locked_socket.write_all(b"Please enter new password:\n").await?;
            };
            let password = {
                let bytes = {
                    let mut locked_socket = shared_socket.lock().await;
                    locked_socket.read(&mut buffer).await?
                };
                String::from_utf8_lossy(&buffer[..bytes]).trim().to_string()
            };
            let pass_hash = hash(password, DEFAULT_COST).expect("Hashing function returned an error");
            let user = UserAccount {
                username,
                pass_hash,
                uuid: Uuid::new_v4().to_string(),
                server_role: "user".to_string(),
            };
            let user_bytes = serde_json::to_vec(&user)
                .expect("Error while serializing user data");
            user_db.insert(user.username.as_bytes(), user_bytes).expect("Database insert error");
            user_db.flush()?;
            {
                let mut locked_socket = shared_socket.lock().await;
                locked_socket.write_all(b"User account created successfully").await?;
            };
            Ok(user)
        }
        "N" | "n" => {
            let mut locked_socket = shared_socket.lock().await;
            locked_socket.write_all(b"Authentication cancelled. Closing connection..").await?;
            anyhow::bail!("Authentication cancelled by user");
        }
        _=> {
            let mut locked_socket = shared_socket.lock().await;
            locked_socket.write_all(b"Invalid response received").await?;
            anyhow::bail!("Invalid response received from user during authentication");
        }
    }
}

async fn server_command_handler(user_db: Arc<Db>, _channel_db: Arc<Db>) {
    let mut reader = BufReader::new(io::stdin()).lines();

    while let Ok(Some(line)) = reader.next_line().await {
        let command = line.trim();

        match command {
            "/add_user" => {
                println!("Enter username:");
                let username = reader.next_line().await.expect("Failed to read username")
                    .unwrap_or_default().trim().to_string();

                match user_db.get(username.as_bytes()).expect("Failed to access user database") {
                    Some(_) => {
                        println!("Username already exist");
                    }
                    None => {
                        println!("Enter password:");
                        let password = reader.next_line().await.expect("Failed to read password")
                            .unwrap_or_default().trim().to_string();

                        println!("Enter user role:");
                        let role = reader.next_line().await.expect("Invalid role provided")
                        .unwrap_or_default().trim().to_string();

                        let uuid = Uuid::new_v4();
                        let pass_hash = hash(password, DEFAULT_COST)
                            .expect("Hashing function returned an error");

                        let user = UserAccount {
                            username: username.clone(),
                            pass_hash,
                            uuid: uuid.to_string(),
                            server_role: role,
                        };
                        let user_bytes = serde_json::to_vec(&user)
                            .expect("Error while serializing user data");
                        user_db.insert(user.username.as_bytes(), user_bytes).expect("Database insert error");
                        println!("User '{}' created successfully", username.clone());
                        let _ = user_db.flush();
                    }
                }
            }
            _=> {
                println!("Unknown command: {}", command);
            }
        }
    }
}

async fn display_channel_history(current_channel: String, channel_db: Arc<Db>, shared_socket: Arc<Mutex<TcpStream>>) -> Result<()> {
    let prefix = format!("{}:", current_channel);
    let mut messages= Vec::new();
    for item in channel_db.scan_prefix(prefix.as_bytes()) {
        let (_, message_bytes) = item?;
        let message: MessageData = serde_json::from_slice(&message_bytes)?;
        messages.push(message);
    }

    messages.sort_by_key(|msg| msg.timestamp);

    for message in messages {
        let json_data = format!("{}{}{}", "JSON:" ,serde_json::to_string(&message)?, "\n");
        let mut locked_socket = shared_socket.lock().await;
        locked_socket.write_all(json_data.as_bytes()).await?;
    }
    
    Ok(())
}

