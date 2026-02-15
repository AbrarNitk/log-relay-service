# Log Streamer Frontend

A modern frontend for streaming logs via SSE using Rust backend.

## Prerequisites
- [Bun](https://bun.sh) (v1.0+)
- Rust (for backend)

## Setup
1. Install dependencies:
   ```bash
   bun install
   ```

## Running
1. Start the Rust backend:
   ```bash
   cd ../crates/server
   cargo run
   ```

2. Start the frontend:
   ```bash
   bun run dev
   ```

3. Open your browser at `http://localhost:3000` (or the port shown by `serve`).

## Features
- **Live Streaming**: Connects to `http://localhost:3000/logs/{run_id}`
- **Filtering**: Client-side filtering of logs by pattern.
- **Aesthetics**: Dark mode, glassmorphism, animations.
