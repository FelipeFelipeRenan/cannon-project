use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> io::Result<()> {
    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:3001".to_string());

    let listener = TcpListener::bind(&address).await?;

    println!("TCP benchmark server listening on {address}");

    loop {
        let (stream, peer) = listener.accept().await?;

        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream).await {
                eprintln!("connection {peer} failed: {error}");
            }
        });
    }
}

async fn handle_connection(mut stream: TcpStream) -> io::Result<()> {
    let mut request = [0u8; 1];

    loop {
        match stream.read_exact(&mut request).await {
            Ok(_) => {
                stream.write_all(&[1u8]).await?;
            }
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                return Ok(());
            }
            Err(error) => {
                return Err(error);
            }
        }
    }
}
