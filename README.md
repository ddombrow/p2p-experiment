# P2P Ops Board

A terminal-based, decentralized task management board built with Rust. This project demonstrates how to build a fully peer-to-peer application using Conflict-free Replicated Data Types (CRDTs) and the `libp2p` network stack.

## Architecture & Distributed Systems Concepts

This application operates without a central server or database. It relies on the following distributed systems concepts:

*   **CRDTs (Conflict-free Replicated Data Types):** State synchronization is handled by [`automerge`](https://automerge.org/). By using CRDTs, multiple peers can concurrently edit the board (adding objectives, changing statuses, appending notes) without centralized conflict resolution. The data structures guarantee eventual consistency across the network.
*   **libp2p Network Stack:**
    *   **mDNS (Multicast DNS):** Used for automatic peer discovery on local networks. Nodes find each other without needing a centralized signaling server or hardcoded IPs.
    *   **Gossipsub:** An efficient pub/sub protocol used to broadcast state changes. When a node mutates the CRDT document, it publishes the resulting bytes to the `/ops-board` topic. Other peers receive the message and merge the changes into their local state.
    *   **Request-Response:** Used for initial state synchronization. When a new node connects to an existing peer, it sends a `SyncRequest`. The established peer responds with the complete bytes of the current CRDT document, allowing the new node to quickly catch up.
    *   **Noise Protocol:** Ensures that all peer-to-peer communication is securely encrypted.
    *   **Yamux:** Multiplexes multiple logical streams (like Gossipsub and Request-Response) over a single underlying TCP connection.
*   **Terminal User Interface (TUI):** Built using `ratatui` and `crossterm` for a responsive, asynchronous terminal experience.

## Getting Started

### Prerequisites

Ensure you have [Rust and Cargo](https://rustup.rs/) installed.

### Running a Node

To start a node, you need to provide a name and optionally a port (defaults to an OS-assigned port if you use `--port 0`).

```bash
# Terminal 1 (Start Alice's node on port 8001)
cargo run -- --name Alice --port 8001

# Terminal 2 (Start Bob's node on port 8002)
cargo run -- --name Bob --port 8002
```

Because of mDNS, the nodes will automatically discover each other on the local network. When they connect, they will synchronize their boards.

### Commands

Once the UI is running, you can use the command line at the bottom to interact with the board.

| Command | Example | Description |
| :--- | :--- | :--- |
| `add "<task>"` | `add "Fix the database router"` | Create a new unassigned objective |
| `assign <n> <operator>` | `assign 1 Bob` | Assign objective `n` to an operator |
| `status <n> <state>` | `status 1 active` | Set status of objective `n` (`active`, `done`, `abort`, `pending`) |
| `take <n>` | `take 1` | Reassign objective `n` to yourself |
| `note <text>` | `note "Router is restarted"` | Append a message to the mission notes |
| `del <n>` | `del 1` | Delete objective `n` |
| `clear` | `clear` | Clear the entire board |
| `help` | `help` | Show the help modal |
| `quit` or `q` | `q` | Exit the application |

*(Use `Esc` or `Enter` to dismiss the help modal)*
