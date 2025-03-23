use std::time::{Duration, Instant};

use anyhow::bail;
use clap::{Args, Parser};
use futures_util::{SinkExt, StreamExt};
use http::Uri;
use tokio::time::interval;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        handshake::client::{Request, generate_key},
        protocol::Message,
    },
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
enum Command {
    Ping(Ping),
    Compression(Compression),
}

#[derive(Args)]
struct Ping {
    url: Uri,
    #[arg(short, long, default_value_t = 1)]
    interval: u64,
    #[arg(short, long, default_value_t = 5)]
    count: u32,
}

#[derive(Args)]
struct Compression {
    url: Uri,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cmd = Command::parse();
    if rustls::crypto::ring::default_provider()
        .install_default()
        .is_err()
    {
        bail!("Failed to install rustls crypto provider")
    }

    match cmd {
        Command::Ping(args) => ping(args).await?,
        Command::Compression(args) => compression(args).await?,
    }

    Ok(())
}

async fn ping(args: Ping) -> anyhow::Result<()> {
    let (mut ws, _) = connect_async(&args.url).await?;
    println!("Connected to {}", args.url);

    let mut latencies = Vec::new();
    let mut interval = interval(Duration::from_secs(args.interval));

    for i in 1..=args.count {
        interval.tick().await;
        let sent_time = Instant::now();
        ws.send(Message::Ping(vec![(i % 256) as u8].into())).await?;

        while let Some(msg) = ws.next().await {
            match msg {
                Ok(Message::Pong(_)) => {
                    let latency = sent_time.elapsed();
                    latencies.push(latency);
                    println!(
                        "Ping {}/{} - Latency: {} ms",
                        i,
                        args.count,
                        latency.as_micros() as f64 / 1000.0
                    );
                    break;
                }
                Ok(_) => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }

    ws.send(Message::Close(None)).await?;
    println!("Connection closed");

    if !latencies.is_empty() {
        let sum: Duration = latencies.iter().sum();
        let avg = sum / latencies.len() as u32;
        let min = latencies.iter().copied().min().unwrap_or_default();
        let max = latencies.iter().copied().max().unwrap_or_default();
        println!("--- {} ping statistics ---", args.url);
        println!(
            "{} pings sent, Min/Avg/Max = {:.2}/{:.2}/{:.2} ms",
            latencies.len(),
            min.as_micros() as f64 / 1000.0,
            avg.as_micros() as f64 / 1000.0,
            max.as_micros() as f64 / 1000.0,
        );
    }

    Ok(())
}

async fn compression(args: Compression) -> anyhow::Result<()> {
    let request = Request::builder()
        .uri(&args.url)
        .header("Host", args.url.host().unwrap_or("localhost"))
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", generate_key())
        // Request the permessage-deflate extension.
        .header("Sec-WebSocket-Extensions", "permessage-deflate")
        .body(())?;

    let (mut ws, response) = connect_async(request).await?;
    println!("Connected to {}", args.url);

    ws.send(Message::Close(None)).await?;
    println!("Connection closed");

    if let Some(extensions) = response.headers().get("Sec-WebSocket-Extensions") {
        println!("Extensions: {extensions:?}");
    } else {
        println!("No extensions in response");
    }

    Ok(())
}
