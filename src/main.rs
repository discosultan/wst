use std::time::{Duration, Instant};

use clap::{Args, Parser};
use futures_util::{SinkExt, StreamExt};
use http::Uri;
use tokio::{
    net::TcpStream,
    time::{interval, timeout},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{
        self,
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
    #[arg(short, long, default_value_t = 5)]
    timeout: u64,
}

#[derive(Args)]
struct Compression {
    url: Uri,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Command::parse() {
        Command::Ping(args) => ping(args).await,
        Command::Compression(args) => compression(args).await,
    }
}

async fn ping(args: Ping) -> anyhow::Result<()> {
    let (mut ws, _) = connect_async(&args.url).await?;
    println!("Connected to {}", args.url);

    let mut latencies = Vec::new();
    let mut interval = interval(Duration::from_secs(args.interval));
    let pong_timeout = Duration::from_secs(args.timeout);

    for i in 1..=args.count {
        interval.tick().await;
        let payload = i.to_be_bytes();
        let sent_time = Instant::now();
        ws.send(Message::Ping(payload.to_vec().into())).await?;

        let pong = timeout(pong_timeout, async {
            while let Some(msg) = ws.next().await {
                match msg {
                    Ok(Message::Pong(p)) if p.as_ref() == payload => {
                        return Ok(sent_time.elapsed());
                    }
                    Ok(_) => continue,
                    Err(e) => return Err(e),
                }
            }
            Err(tungstenite::error::Error::AlreadyClosed)
        })
        .await;

        match pong {
            Ok(Ok(latency)) => {
                latencies.push(latency);
                println!("Ping {}/{} - Latency: {:.2} ms", i, args.count, ms(latency));
            }
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => println!("Ping {}/{} - Timeout", i, args.count),
        }
    }

    close(&mut ws).await?;
    println!("Connection closed");

    if !latencies.is_empty() {
        let sum: Duration = latencies.iter().sum();
        let avg = sum / latencies.len() as u32;
        let min = latencies.iter().copied().min().unwrap();
        let max = latencies.iter().copied().max().unwrap();
        println!("--- {} ping statistics ---", args.url);
        println!(
            "{} pings sent, Min/Avg/Max = {:.2}/{:.2}/{:.2} ms",
            latencies.len(),
            ms(min),
            ms(avg),
            ms(max),
        );
    }

    Ok(())
}

fn ms(d: Duration) -> f64 {
    d.as_micros() as f64 / 1000.0
}

async fn close(ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>) -> tungstenite::Result<()> {
    ws.send(Message::Close(None)).await?;
    // Drain until the server's close ack ends the stream, bounded so a
    // misbehaving peer can't hang shutdown.
    let _ = timeout(Duration::from_secs(5), async {
        while ws.next().await.is_some() {}
    })
    .await;
    Ok(())
}

async fn compression(args: Compression) -> anyhow::Result<()> {
    let host = args
        .url
        .authority()
        .map(|a| a.as_str())
        .unwrap_or("localhost");
    let request = Request::builder()
        .uri(&args.url)
        .header("Host", host)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", generate_key())
        // Request the permessage-deflate extension.
        .header("Sec-WebSocket-Extensions", "permessage-deflate")
        .body(())?;

    let (mut ws, response) = connect_async(request).await?;
    println!("Connected to {}", args.url);

    close(&mut ws).await?;
    println!("Connection closed");

    match response.headers().get("Sec-WebSocket-Extensions") {
        Some(extensions) => {
            let value = extensions.to_str().unwrap_or("");
            let deflate = value.split(',').any(|ext| {
                ext.trim().split(';').next().unwrap_or("").trim() == "permessage-deflate"
            });
            println!("Extensions: {value}");
            println!(
                "Compression: {}",
                if deflate { "enabled" } else { "disabled" },
            );
        }
        None => {
            println!("No extensions in response");
            println!("Compression: disabled");
        }
    }

    Ok(())
}
