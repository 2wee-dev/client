# 2Wee Client

A keyboard-driven TUI client for [2Wee](https://2wee.dev) applications. Connects to any server implementing the 2Wee protocol and renders a full data entry interface in your terminal — no mouse required.

## Quick start

Download the binary for your platform from [Releases](https://github.com/2wee-dev/client/releases/latest):

| Platform | Binary |
|----------|--------|
| macOS (Apple Silicon) | `two_wee_client-macos-arm64` |
| macOS (Intel) | `two_wee_client-macos-x86_64` |
| Linux x86_64 | `two_wee_client-linux-x86_64` |
| Linux ARM64 | `two_wee_client-linux-arm64` |

```bash
chmod +x two_wee_client-macos-arm64
./two_wee_client-macos-arm64 https://your-app.example.com/terminal
```

## Usage

```bash
two_wee_client <server-url>

# Or set the server via environment variable
TWO_WEE_SERVER=https://your-app.example.com/terminal two_wee_client
```

The client connects to the server, fetches the main menu, and renders the full TUI. Use arrow keys and keyboard shortcuts to navigate.

## Web terminal

To run the client in a browser without installing anything locally, use [2Wee Web Terminal](https://github.com/2wee-dev/web-terminal). It runs `two_wee_client` server-side and streams the session over WebSocket.

## Build from source

```bash
git clone https://github.com/2wee-dev/client.git
cd client
cargo build --release
```

## Documentation

Full documentation at [2wee.dev](https://2wee.dev/client/overview).
