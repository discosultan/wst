use std::{str::FromStr, time::Instant};

use anyhow::bail;
use clap::{Args, Parser};
use futures_util::{SinkExt, StreamExt};
use http::Uri;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
enum Command {
    Ping(Ping),
}

#[derive(Args)]
struct Ping {
    url: String,
    #[arg(short, long, default_value_t = 1)]
    interval: u64,
    #[arg(short, long, default_value_t = 5)]
    count: u32,
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
    }

    Ok(())
}

async fn ping(args: Ping) -> anyhow::Result<()> {
    let url = Uri::from_str(&args.url)?;

    let (ws_stream, _) = connect_async(url).await?;
    println!("Connected to {}", args.url);

    let (mut write, mut read) = ws_stream.split();
    let mut latencies = Vec::new();

    for i in 1..=args.count {
        let sent_time = Instant::now();
        write.send(Message::Ping(vec![i as u8].into())).await?;

        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Pong(_)) => {
                    let latency_ms = sent_time.elapsed().as_micros() as f64 / 1000.0;
                    println!("Ping {}/{} - Latency: {} ms", i, args.count, latency_ms);
                    latencies.push(latency_ms);
                    break;
                }
                Ok(_) => continue,
                Err(e) => return Err(e.into()),
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(args.interval)).await;
    }

    write.send(Message::Close(None)).await?;
    if !latencies.is_empty() {
        let avg = latencies.iter().sum::<f64>() / latencies.len() as f64;
        let min = latencies.iter().copied().fold(f64::INFINITY, f64::min);
        let max = latencies.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        println!("--- {} ping statistics ---", args.url);
        println!(
            "{} pings sent, Min/Avg/Max = {:.2}/{:.2}/{:.2} ms",
            latencies.len(),
            min,
            avg,
            max
        );
    }
    println!("Connection closed");

    Ok(())
}
