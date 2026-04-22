mod doc;
mod tui;

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    io,
    time::Duration,
};

use async_trait::async_trait;
use bytes::Bytes;
use clap::Parser;
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyModifiers,
};
use futures::{AsyncReadExt as _, AsyncWriteExt as _, StreamExt};
use libp2p::{
    Multiaddr, StreamProtocol, gossipsub, identify, mdns,
    request_response::{self, ProtocolSupport},
    swarm::{NetworkBehaviour, SwarmEvent},
};
use prost::Message;
use tui::{Command, parse_command};

// --- Protobuf generated types ---
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/sync.rs"));
}

// --- CLI ---

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "0")]
    port: u16,
    #[arg(long)]
    name: String,
    #[arg(long)]
    file: Option<String>,
    #[arg(long)]
    topic: Option<String>,
}

// --- Request/response sync protocol (Protobuf codec) ---

type SyncRequest = proto::SyncRequest;
type SyncResponse = proto::SyncResponse;

#[derive(Clone, Default)]
struct SyncCodec;

#[async_trait]
impl request_response::Codec for SyncCodec {
    type Protocol = StreamProtocol;
    type Request = SyncRequest;
    type Response = SyncResponse;

    async fn read_request<T: Send + Unpin + futures::AsyncRead>(
        &mut self,
        _: &StreamProtocol,
        io: &mut T,
    ) -> io::Result<Self::Request> {
        let mut buf = Vec::new();
        unsigned_varint::aio::read_usize(&mut *io)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        io.read_to_end(&mut buf).await?;
        SyncRequest::decode(Bytes::from(buf))
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    async fn read_response<T: Send + Unpin + futures::AsyncRead>(
        &mut self,
        _: &StreamProtocol,
        io: &mut T,
    ) -> io::Result<Self::Response> {
        let mut buf = Vec::new();
        unsigned_varint::aio::read_usize(&mut *io)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        io.read_to_end(&mut buf).await?;
        SyncResponse::decode(Bytes::from(buf))
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    async fn write_request<T: Send + Unpin + futures::AsyncWrite>(
        &mut self,
        _: &StreamProtocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()> {
        let buf = req.encode_to_vec();
        let mut varint_buf = unsigned_varint::encode::usize_buffer();
        io.write_all(unsigned_varint::encode::usize(buf.len(), &mut varint_buf))
            .await?;
        io.write_all(&buf).await?;
        io.close().await
    }

    async fn write_response<T: Send + Unpin + futures::AsyncWrite>(
        &mut self,
        _: &StreamProtocol,
        io: &mut T,
        resp: Self::Response,
    ) -> io::Result<()> {
        let buf = resp.encode_to_vec();
        let mut varint_buf = unsigned_varint::encode::usize_buffer();
        io.write_all(unsigned_varint::encode::usize(buf.len(), &mut varint_buf))
            .await?;
        io.write_all(&buf).await?;
        io.close().await
    }
}

// --- Combined network behaviour ---

#[derive(NetworkBehaviour)]
struct Behaviour {
    mdns: mdns::tokio::Behaviour,
    gossipsub: gossipsub::Behaviour,
    identify: identify::Behaviour,
    sync: request_response::Behaviour<SyncCodec>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let topic_str = args.topic.unwrap_or_else(|| {
        let pet = petname::petname(2, "-").unwrap_or_else(|| "random-board".to_string());
        format!("opsboard-{}", pet)
    });

    // Build swarm
    let mut swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_quic()
        .with_behaviour(
            |key| -> Result<Behaviour, Box<dyn std::error::Error + Send + Sync>> {
                let mdns = mdns::tokio::Behaviour::new(
                    mdns::Config::default(),
                    key.public().to_peer_id(),
                )?;

                let message_id_fn = |msg: &gossipsub::Message| {
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    msg.data.hash(&mut h);
                    gossipsub::MessageId::from(h.finish().to_string())
                };
                let gs_config = gossipsub::ConfigBuilder::default()
                    .heartbeat_interval(Duration::from_secs(5))
                    .validation_mode(gossipsub::ValidationMode::Strict)
                    .message_id_fn(message_id_fn)
                    .build()
                    .map_err(Box::<dyn std::error::Error + Send + Sync>::from)?;
                let gossipsub = gossipsub::Behaviour::new(
                    gossipsub::MessageAuthenticity::Signed(key.clone()),
                    gs_config,
                )?;

                let identify = identify::Behaviour::new(
                    identify::Config::new(format!("/opsboard/1.0.0/{}", topic_str), key.public())
                        .with_agent_version(args.name.clone()),
                );

                let sync = request_response::Behaviour::<SyncCodec>::new(
                    [(
                        StreamProtocol::new("/opsboard/sync/1"),
                        ProtocolSupport::Full,
                    )],
                    request_response::Config::default(),
                );

                Ok(Behaviour {
                    mdns,
                    gossipsub,
                    identify,
                    sync,
                })
            },
        )?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    let topic = gossipsub::IdentTopic::new(topic_str.clone());
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
    swarm.listen_on(format!("/ip4/0.0.0.0/udp/{}/quic-v1", args.port).parse::<Multiaddr>()?)?;

    // App state
    let mut app = tui::App::new(args.name.clone(), topic_str.clone());

    if let Some(file_path) = &args.file {
        let contents = std::fs::read_to_string(file_path)?;
        for line in contents.lines() {
            let task = line.trim();
            if !task.is_empty() {
                app.doc.add_objective(task, "unassigned");
            }
        }
        app.push_log(format!("ingested objectives from {}", file_path));
    }

    // Add join message to the document
    app.doc
        .add_system_event(&format!("{} joined the board", args.name));
    rebuild_comms_log(&mut app);

    // Terminal
    let mut terminal = ratatui::init();
    crossterm::execute!(std::io::stdout(), EnableMouseCapture)?;

    let result = run(&mut terminal, &mut app, &mut swarm, &topic, &args.name).await;

    crossterm::execute!(std::io::stdout(), DisableMouseCapture).ok();
    ratatui::restore();
    result
}

async fn run(
    terminal: &mut ratatui::Terminal<ratatui::prelude::CrosstermBackend<std::io::Stdout>>,
    app: &mut tui::App,
    swarm: &mut libp2p::Swarm<Behaviour>,
    topic: &gossipsub::IdentTopic,
    operator: &str,
) -> anyhow::Result<()> {
    let mut events = EventStream::new();
    let mut peer_map: HashMap<libp2p::PeerId, String> = HashMap::new();

    loop {
        terminal.draw(|f| tui::render(f, app))?;

        let flash_sleep = async {
            let mention_next = app.mention_bell.and_then(|t| {
                let elapsed = t.elapsed();
                let ms = elapsed.as_millis();
                let next_ms: u64 = if ms < 200 {
                    200
                } else if ms < 350 {
                    350
                } else if ms < 600 {
                    600
                } else {
                    return None;
                };
                std::time::Duration::from_millis(next_ms).checked_sub(elapsed)
            });
            let deadlines = [
                app.copy_flash
                    .and_then(|t| std::time::Duration::from_millis(300).checked_sub(t.elapsed())),
                mention_next,
            ];
            match deadlines.iter().flatten().copied().min() {
                Some(d) => tokio::time::sleep(d).await,
                None => std::future::pending::<()>().await,
            }
        };

        tokio::select! {
            swarm_event = swarm.select_next_some() => {
                handle_swarm(swarm_event, app, swarm, topic, &mut peer_map, operator)?;
            }
            Some(Ok(term_event)) = events.next() => {
                if handle_input(term_event, app, swarm, topic, operator)? {
                    break;
                }
            }
            _ = flash_sleep => {
                if app.copy_flash.map(|t| t.elapsed() >= std::time::Duration::from_millis(300)).unwrap_or(false) {
                    app.copy_flash = None;
                }
                if app.mention_bell.map(|t| t.elapsed() >= std::time::Duration::from_millis(600)).unwrap_or(false) {
                    app.mention_bell = None;
                }
            }
        }
    }

    Ok(())
}

fn handle_swarm(
    event: SwarmEvent<BehaviourEvent>,
    app: &mut tui::App,
    swarm: &mut libp2p::Swarm<Behaviour>,
    topic: &gossipsub::IdentTopic,
    peer_map: &mut HashMap<libp2p::PeerId, String>,
    operator: &str,
) -> anyhow::Result<()> {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => {
            app.push_log(format!("listening on {address}"));
        }

        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
            app.push_log(format!("connected: {peer_id}"));
            swarm.behaviour_mut().sync.send_request(
                &peer_id,
                SyncRequest {
                    topic: app.topic.clone(),
                },
            );
        }

        SwarmEvent::ConnectionClosed {
            peer_id,
            num_established,
            ..
        } => {
            if num_established == 0
                && let Some(name) = peer_map.remove(&peer_id)
            {
                app.push_log(format!("disconnected: {name}"));
                if let Some(peer) = app.peers.iter_mut().find(|p| p.name == name) {
                    peer.online = false;
                }
            }
        }

        SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Discovered(peers))) => {
            for (peer_id, addr) in peers {
                swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                if !swarm.is_connected(&peer_id)
                    && let Err(e) = swarm.dial(addr)
                {
                    app.push_log(format!("dial error: {e}"));
                }
            }
        }

        SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Expired(peers))) => {
            for (peer_id, _) in peers {
                swarm
                    .behaviour_mut()
                    .gossipsub
                    .remove_explicit_peer(&peer_id);
            }
        }

        SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Message {
            message,
            ..
        })) => {
            if let Err(e) = app.doc.merge_bytes(&message.data) {
                app.push_log(format!("merge error: {e}"));
            } else {
                let prev_len = app.comms_log.len();
                rebuild_comms_log(app);
                check_mentions(app, prev_len, operator);
            }
        }

        SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received {
            peer_id,
            info,
            ..
        })) => {
            let expected_protocol = format!("/opsboard/1.0.0/{}", app.topic);
            let is_ops_peer = info.protocol_version == expected_protocol;
            if is_ops_peer {
                let name = info.agent_version.clone();
                let was_online = app.peers.iter().any(|p| p.name == name && p.online);
                peer_map.insert(peer_id, name.clone());
                if let Some(peer) = app.peers.iter_mut().find(|p| p.name == name) {
                    peer.online = true;
                } else {
                    app.peers.push(tui::Peer {
                        name: name.clone(),
                        online: true,
                    });
                }
                if !was_online {
                    app.push_log(format!("online: {name}"));
                }
            }
        }

        SwarmEvent::Behaviour(BehaviourEvent::Sync(request_response::Event::Message {
            peer: _,
            message:
                request_response::Message::Request {
                    request, channel, ..
                },
            ..
        })) => {
            if request.topic == app.topic {
                let doc_bytes = app.doc.save();
                swarm
                    .behaviour_mut()
                    .sync
                    .send_response(channel, SyncResponse { doc_bytes })
                    .ok();
            }
        }

        SwarmEvent::Behaviour(BehaviourEvent::Sync(request_response::Event::Message {
            message: request_response::Message::Response { response, .. },
            ..
        })) => {
            if let Err(e) = app.doc.merge_bytes(&response.doc_bytes) {
                app.push_log(format!("sync error: {e}"));
            } else {
                let prev_len = app.comms_log.len();
                rebuild_comms_log(app);
                check_mentions(app, prev_len, operator);
                app.push_log("synced doc from peer".to_string());
                // Re-broadcast after sync: gossipsub mesh is now established and
                // our join event may have raced with the initial sync exchange.
                let bytes = app.doc.save();
                if let Err(e) = swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(topic.clone(), bytes)
                {
                    if !matches!(e, libp2p::gossipsub::PublishError::Duplicate) {
                        app.push_log(format!("publish error: {e}"));
                    }
                }
            }
        }

        _ => {}
    }
    Ok(())
}

fn handle_input(
    event: Event,
    app: &mut tui::App,
    swarm: &mut libp2p::Swarm<Behaviour>,
    topic: &gossipsub::IdentTopic,
    operator: &str,
) -> anyhow::Result<bool> {
    if let Event::Mouse(mouse_event) = event {
        if mouse_event.kind
            == crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left)
            && tui::is_copy_button_clicked(app, mouse_event.column, mouse_event.row)
        {
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                if clipboard.set_text(app.topic.clone()).is_ok() {
                    app.push_log("copied topic to clipboard");
                    app.copy_flash = Some(std::time::Instant::now());
                } else {
                    app.push_log("failed to copy to clipboard");
                }
            } else {
                app.push_log("failed to access clipboard");
            }
        }
        return Ok(false);
    }

    let Event::Key(key) = event else {
        return Ok(false);
    };

    // Dismiss help modal with Esc or Enter
    if app.show_help {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                app.show_help = false;
            }
            _ => {}
        }
        return Ok(false);
    }

    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return Ok(true);
        }
        KeyCode::Char(c) => {
            app.input.push(c);
        }
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Enter => {
            let input = std::mem::take(&mut app.input);
            if execute_command(parse_command(&input), app, swarm, topic, operator)? {
                return Ok(true);
            }
        }
        KeyCode::Esc => {
            app.input.clear();
        }
        _ => {}
    }

    Ok(false)
}

fn render_asciidoc(board: &doc::Board, topic: &str, operator: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("= Ops Board: {topic}\n"));
    out.push_str(&format!(":operator: {operator}\n\n"));

    out.push_str("== Objectives\n\n");
    if board.objectives.is_empty() {
        out.push_str("_No objectives._\n\n");
    } else {
        out.push_str(
            "[cols=\">1,^1,<4,<2\", options=\"header\"]\n|===\n| # | Status | Task | Assignee\n\n",
        );
        for (i, obj) in board.objectives.iter().enumerate() {
            out.push_str(&format!(
                "| {} | {} | {} | {}\n",
                i + 1,
                obj.status.as_str(),
                obj.task,
                obj.assignee
            ));
        }
        out.push_str("|===\n\n");
    }

    out.push_str("== Comms\n\n");
    if board.messages.is_empty() {
        out.push_str("_No messages._\n");
    } else {
        for msg in &board.messages {
            match msg.kind {
                doc::MessageKind::Message => {
                    out.push_str(&format!(
                        "`[{}]` *{}*: {}\n\n",
                        msg.timestamp, msg.author, msg.text
                    ));
                }
                doc::MessageKind::System => {
                    out.push_str(&format!("`[{}]` _{}_\n\n", msg.timestamp, msg.text));
                }
            }
        }
    }
    out
}

fn check_mentions(app: &mut tui::App, prev_len: usize, operator: &str) {
    let mention = format!("@{}", operator.to_lowercase());
    let triggered = app.comms_log[prev_len..].iter().any(|e| {
        matches!(&e.kind, tui::CommsKind::Message { text, .. }
            if text.to_lowercase().contains(&mention)
            || text.to_lowercase().contains("@all"))
    });
    if triggered {
        app.mention_bell = Some(std::time::Instant::now());
    }
}

fn rebuild_comms_log(app: &mut tui::App) {
    app.comms_log = app
        .doc
        .read()
        .messages
        .into_iter()
        .map(|msg| {
            let kind = match msg.kind {
                doc::MessageKind::System => tui::CommsKind::System(msg.text),
                doc::MessageKind::Message => tui::CommsKind::Message {
                    author: msg.author,
                    text: msg.text,
                },
            };
            tui::CommsEntry {
                timestamp: msg.timestamp,
                kind,
            }
        })
        .collect();
}

fn execute_command(
    cmd: Command,
    app: &mut tui::App,
    swarm: &mut libp2p::Swarm<Behaviour>,
    topic: &gossipsub::IdentTopic,
    operator: &str,
) -> anyhow::Result<bool> {
    let bytes = match cmd {
        Command::Add { task } => {
            app.push_log(format!("added \"{task}\""));
            Some(app.doc.add_objective(&task, "unassigned"))
        }
        Command::Assign { index, assignee } => {
            let board = app.doc.read();
            if index >= board.objectives.len() {
                app.push_log(format!("no objective [{}]", index + 1));
                return Ok(false);
            }
            let known: Vec<String> = std::iter::once(operator.to_string())
                .chain(app.peers.iter().map(|p| p.name.clone()))
                .collect();
            let matched = known
                .iter()
                .find(|n| n.to_lowercase().starts_with(&assignee.to_lowercase()))
                .cloned();
            match matched {
                Some(name) => {
                    app.push_log(format!("assigned [{}] to {name}", index + 1));
                    Some(app.doc.take_objective(index, &name))
                }
                None => {
                    app.push_log(format!(
                        "unknown operator \"{assignee}\" — known: {}",
                        known.join(", ")
                    ));
                    None
                }
            }
        }
        Command::Status { index, status } => {
            let len = app.doc.read().objectives.len();
            if index >= len {
                app.push_log(format!("no objective [{}]", index + 1));
                return Ok(false);
            }
            app.push_log(format!("set [{}] -> {status}", index + 1));
            Some(app.doc.set_status(index, &status))
        }
        Command::Take { index } => {
            let len = app.doc.read().objectives.len();
            if index >= len {
                app.push_log(format!("no objective [{}]", index + 1));
                return Ok(false);
            }
            app.push_log(format!("took [{}]", index + 1));
            Some(app.doc.take_objective(index, operator))
        }
        Command::Delete { index } => {
            let len = app.doc.read().objectives.len();
            if index >= len {
                app.push_log(format!("no objective [{}]", index + 1));
                return Ok(false);
            }
            app.push_log(format!("deleted [{}]", index + 1));
            Some(app.doc.delete_objective(index))
        }
        Command::Msg { text } => {
            let bytes = app.doc.add_message(operator, &text);
            rebuild_comms_log(app);
            Some(bytes)
        }
        Command::Clear => {
            app.doc = crate::doc::Doc::new();
            app.push_log("document cleared");
            Some(app.doc.save())
        }
        Command::Help => {
            app.show_help = true;
            None
        }
        Command::Quit => {
            let bytes = app
                .doc
                .add_system_event(&format!("{operator} left the board"));
            rebuild_comms_log(app);
            if let Err(e) = swarm
                .behaviour_mut()
                .gossipsub
                .publish(topic.clone(), bytes)
            {
                if !matches!(e, libp2p::gossipsub::PublishError::Duplicate) {
                    app.push_log(format!("publish error: {e}"));
                }
            }
            if let Err(e) = std::fs::create_dir_all("output") {
                app.push_log(format!("output dir error: {e}"));
            } else {
                let stem = format!("output/{}-{}", app.topic, operator);
                if let Err(e) = std::fs::write(format!("{stem}.automerge"), app.doc.save()) {
                    app.push_log(format!("save error: {e}"));
                } else {
                    app.push_log(format!("saved session to {stem}.automerge"));
                }
                let board = app.doc.read();
                if let Err(e) = std::fs::write(
                    format!("{stem}.adoc"),
                    render_asciidoc(&board, &app.topic, operator),
                ) {
                    app.push_log(format!("adoc save error: {e}"));
                }
            }
            return Ok(true);
        }
        Command::Unknown(s) => {
            if !s.is_empty() {
                app.push_log(format!("unknown command: {s}"));
            }
            None
        }
    };

    if let Some(bytes) = bytes
        && let Err(e) = swarm
            .behaviour_mut()
            .gossipsub
            .publish(topic.clone(), bytes)
    {
        if !matches!(e, libp2p::gossipsub::PublishError::Duplicate) {
            app.push_log(format!("publish error: {e}"));
        }
    }

    Ok(false)
}
