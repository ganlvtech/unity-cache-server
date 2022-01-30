use std::path::PathBuf;
use std::time::Duration;

use tokio::io::BufReader;
use tokio::io::BufWriter;
use tokio::net::TcpListener;
use tokio::time::sleep;

use unity_cache_server::handle;
use unity_cache_server::handlers::FileSystemHandler;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:8126").await?;
    let mut fs_handler = FileSystemHandler::new(PathBuf::from(".cache_fs"), PathBuf::from(".cache_fs"));
    fs_handler.set_max_file_size(256 * 1024 * 1024);

    loop {
        match listener.accept().await {
            Ok((mut conn, addr)) => {
                println!("Accept connection from {}", addr);
                let handler = fs_handler.clone();
                tokio::spawn(async move {
                    let (reader, writer) = conn.split();
                    let mut reader = BufReader::new(reader);
                    let mut writer = BufWriter::new(writer);
                    match handle(&mut reader, &mut writer, handler).await {
                        Ok(_) => {
                            println!("Client {} quit", addr);
                        }
                        Err(e) => {
                            println!("Client {} disconnect with error: {:?}", addr, e);
                        }
                    }
                });
            }
            Err(e) => {
                println!("Accept connection error: {:?}", e);
                sleep(Duration::from_secs(1)).await;
            }
        }
    }
}
