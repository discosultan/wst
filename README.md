# wst

WebSocket Tools — a small CLI for probing WebSocket endpoints.

## Install

```sh
cargo install --path .
```

## Usage

Measure round-trip latency via WebSocket ping/pong frames:

```sh
wst ping wss://example.com/ws
wst ping wss://example.com/ws --count 10 --interval 1 --timeout 5
```

Check whether a server negotiates the `permessage-deflate` extension:

```sh
wst compression wss://example.com/ws
```
