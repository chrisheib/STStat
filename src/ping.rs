use color_eyre::eyre::eyre;
use color_eyre::Result;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::timeout;

pub async fn tcp_ping_tokio(target_addr: &str, port: u16) -> Result<Duration> {
    let addr: SocketAddr = format!("{target_addr}:{port}").parse()?;

    let start_time = tokio::time::Instant::now();

    let connect_future = TcpStream::connect(&addr);

    // Set a timeout for the connection attempt
    match timeout(Duration::from_secs(1), connect_future).await {
        // Connection successful, close it immediately
        Ok(Ok(mut stream)) => {
            let _ = stream.shutdown().await;
            Ok(start_time.elapsed())
        }
        // Future returned on time but failed to connect
        Ok(Err(e)) => Err(eyre!("Failed to connect to {addr}: {e}")),
        // Timeout occurred
        Err(_) => Err(eyre!("Connection to {addr} timed out.")),
    }
}

#[tokio::test]
pub async fn dbg_ping_main() -> Result<()> {
    for i in 0..10 {
        eprintln!("Ping test iteration: {}", i);
        let google_dns = "8.8.8.8";
        let dns_port = 53;

        eprintln!(
            "Checking TCP connectivity to {}:{} (using tokio)...",
            google_dns, dns_port
        );
        match tcp_ping_tokio(google_dns, dns_port).await {
            Ok(rtt) => eprintln!("Connected to {}:{}. RTT: {:?}", google_dns, dns_port, rtt),
            Err(e) => eprintln!("Failed to connect to {}:{}: {}", google_dns, dns_port, e),
        }

        eprintln!("Ping test completed.");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Ok(())
}
