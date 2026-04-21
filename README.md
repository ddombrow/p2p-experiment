# P2P Ops Board

A terminal-based, decentralized task management board built with Rust. Operators can spin up an isolated mission board from a plaintext file and share the generated topic slug with their team — no server, no infrastructure, no single point of failure.

## Architecture & Distributed Systems Concepts

This application operates without a central server or database. It relies on the following distributed systems concepts:

*   **CRDTs (Conflict-free Replicated Data Types):** State synchronization is handled by [`automerge`](https://automerge.org/). Multiple peers can concurrently edit the board (adding objectives, changing statuses, appending notes) without centralized conflict resolution. The data structures guarantee eventual consistency across the network.
*   **libp2p Network Stack:**
    *   **mDNS (Multicast DNS):** Used for automatic peer discovery on local networks. Nodes find each other without a centralized signaling server or hardcoded IPs.
    *   **Gossipsub:** An efficient pub/sub protocol used to broadcast incremental state changes. When a node mutates the CRDT document, it publishes the resulting bytes to the active topic. Peers receive the message and merge changes into their local state. Only peers subscribed to the same topic receive updates.
    *   **Identify:** Used for topic-scoped peer validation. Each node advertises its unique mission topic as its protocol version. Nodes reject peers that don't broadcast an exact topic match, preventing any cross-topic state bleed on the same local network.
    *   **Request-Response:** Used for full document sync when a new node first connects. The joining node sends a `SyncRequest` containing its topic; the established peer validates the topic and responds with the complete CRDT document bytes so the new node catches up instantly.
    *   **Noise Protocol:** All peer-to-peer communication is encrypted.
    *   **Yamux:** Multiplexes Gossipsub, Identify, and Request-Response over a single TCP connection.
*   **Terminal User Interface (TUI):** Built using `ratatui` and `crossterm` for a responsive, async terminal experience with mouse support.

## Getting Started

### Prerequisites

Ensure you have [Rust and Cargo](https://rustup.rs/) installed.

### Running a Node

There are two modes of operation: **Leader** (creates the board) and **Operative** (joins an existing board).

**1. Leader — start a board from a mission file:**
```bash
# Ingests one task per line. Generates a unique topic (e.g. ops-board-a1b2c3).
# The topic is shown in the status bar — click [Copy] to copy it to your clipboard.
cargo run -- --name Alice --file example-boards/red-team.txt
```

**2. Operative — join an existing board by topic:**
```bash
# Joins Alice's board. mDNS discovers her automatically on the LAN.
# The full CRDT document is synced on connect.
cargo run -- --name Bob --topic ops-board-a1b2c3
```

> **Tip:** The `--port` flag is optional. If omitted, an ephemeral port is assigned automatically.

### Example Board Files

Pre-built mission files are included in `example-boards/`:

| File | Description |
| :--- | :--- |
| `red-team.txt` | Adversarial red team operation objectives |
| `incident-response.txt` | Security incident response runbook |
| `infrastructure.txt` | Infrastructure provisioning tasks |

Each file is plaintext, one task per line — easy to write ahead of time and share out-of-band.

### Commands

Once the UI is running, use the command bar at the bottom to interact with the board.

| Command | Example | Description |
| :--- | :--- | :--- |
| `add "<task>"` | `add "Fix the database router"` | Create a new unassigned objective |
| `assign <n> <operator>` | `assign 1 Bob` | Assign objective `n` to an operator |
| `status <n> <state>` | `status 1 active` | Set status of objective `n` (`active`, `done`, `abort`, `pending`) |
| `take <n>` | `take 1` | Reassign objective `n` to yourself |
| `note <text>` | `note "Router is back up"` | Append a message to the mission notes |
| `del <n>` | `del 1` | Delete objective `n` |
| `clear` | `clear` | Clear the entire board |
| `help` | `help` | Show the help modal |
| `quit` or `q` | `q` | Exit the application |

*(Use `Esc` or `Enter` to dismiss the help modal)*

### Status Colors

Objectives are color-coded for fast at-a-glance situational awareness:

| Status | Color | Meaning |
| :--- | :--- | :--- |
| `PENDING` | Gray | Unstarted, awaiting assignment |
| `ACTIVE` | **Yellow** | In progress |
| `DONE` | **Green** | Completed |
| `ABORT` | **Red** | Cancelled or failed |
