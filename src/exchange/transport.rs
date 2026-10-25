use super::replica::{Change, SharedState};
use crate::model::identifier;
use anyhow::{bail, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};
use tungstenite::{
    client_tls_with_config, protocol::WebSocketConfig, stream::MaybeTlsStream, Message, WebSocket,
};

const MAX_PACKET: usize = 128 * 1024 * 1024;
const MAX_PDF: usize = 64 * 1024 * 1024;
const CHUNK: usize = 32 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Packet {
    NeedState,
    Snapshot {
        state: SharedState,
        source: Option<Source>,
    },
    Ops {
        document: String,
        changes: Vec<Change>,
    },
}
#[derive(Serialize, Deserialize)]
struct Source {
    sha256: String,
    data: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Chunk {
    transfer: String,
    part: usize,
    total: usize,
    data: String,
}
pub enum Command {
    NeedState(String),
    Snapshot {
        to: Option<String>,
        state: SharedState,
        source: Option<PathBuf>,
    },
    Ops {
        document: String,
        changes: Vec<Change>,
    },
}
pub enum Event {
    Connected {
        code: String,
        peers: Vec<String>,
    },
    Joined(String),
    Left(String),
    NeedState(String),
    Snapshot {
        state: SharedState,
        source: Option<String>,
    },
    Ops {
        document: String,
        changes: Vec<Change>,
    },
    Status(String),
    Ended(String),
}
pub struct Link {
    commands: mpsc::SyncSender<Command>,
    pub events: mpsc::Receiver<Event>,
    stop: Arc<AtomicBool>,
    restart: Arc<AtomicBool>,
}
impl Drop for Link {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
impl Link {
    pub fn start(base: &str, code: Option<String>, root: PathBuf) -> Result<Self> {
        let url = endpoint(base)?;
        let (commands, rx) = mpsc::sync_channel(8);
        let (tx, events) = mpsc::sync_channel(8);
        let stop = Arc::new(AtomicBool::new(false));
        let restart = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker_restart = restart.clone();
        thread::spawn(move || worker(url, code, root, rx, tx, worker_stop, worker_restart));
        Ok(Self {
            commands,
            events,
            stop,
            restart,
        })
    }
    pub fn send(&self, command: Command) {
        if self.commands.try_send(command).is_err() {
            // Local replica remains authoritative. Reconnect exchanges complete
            // states, recovering every edit when bounded queues overflow.
            self.restart.store(true, Ordering::Relaxed);
        }
    }
}
pub fn endpoint(base: &str) -> Result<url::Url> {
    let mut u =
        url::Url::parse(base.trim()).context("Enter an http:// or https:// exchange server URL")?;
    ensure!(
        ["http", "https", "ws", "wss"].contains(&u.scheme()) && u.host_str().is_some(),
        "Use an HTTP(S) exchange server URL"
    );
    ensure!(
        u.username().is_empty()
            && u.password().is_none()
            && u.query().is_none()
            && u.fragment().is_none(),
        "Server URL must not contain a password, query or fragment"
    );
    let secure = ["https", "wss"].contains(&u.scheme());
    u.set_scheme(if secure { "wss" } else { "ws" })
        .map_err(|_| anyhow::anyhow!("Invalid URL"))?;
    let path = u.path().trim_end_matches('/');
    if !path.ends_with("/v1/exchange") {
        u.set_path(&format!("{path}/v1/exchange"));
    }
    Ok(u)
}
pub fn room_code(text: &str) -> Result<String> {
    let code = text.trim().to_ascii_uppercase();
    ensure!(
        code.len() == 5
            && code
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()),
        "Enter the five-letter or digit sharing code"
    );
    Ok(code)
}
fn socket(url: &url::Url) -> Result<WebSocket<MaybeTlsStream<TcpStream>>> {
    let addresses = (
        url.host_str()
            .context("Missing server host")?
            .trim_start_matches('[')
            .trim_end_matches(']'),
        url.port_or_known_default().context("Missing server port")?,
    )
        .to_socket_addrs()?;
    let mut tcp = None;
    for addr in addresses.take(8) {
        if let Ok(stream) = TcpStream::connect_timeout(&addr, Duration::from_secs(4)) {
            tcp = Some(stream);
            break;
        }
    }
    let tcp = tcp.context("Cannot connect to exchange server")?;
    tcp.set_nodelay(true)?;
    tcp.set_read_timeout(Some(Duration::from_secs(8)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(8)))?;
    let config = WebSocketConfig::default()
        .write_buffer_size(0)
        .max_write_buffer_size(256 * 1024)
        .max_message_size(Some(128 * 1024))
        .max_frame_size(Some(128 * 1024));
    let (mut ws, _) = client_tls_with_config(url.as_str(), tcp, Some(config), None)?;
    let stream = match ws.get_mut() {
        MaybeTlsStream::Plain(s) => s,
        MaybeTlsStream::Rustls(s) => &mut s.sock,
        _ => bail!("Unsupported TLS transport"),
    };
    stream.set_read_timeout(Some(Duration::from_millis(15)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    Ok(ws)
}
fn encode(packet: Packet, to: Option<String>) -> Result<VecDeque<String>> {
    let raw = serde_json::to_vec(&packet)?;
    ensure!(
        raw.len() <= MAX_PACKET,
        "Shared document exceeds the 128 MiB transfer limit"
    );
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&raw)?;
    let bytes = encoder.finish()?;
    let total = bytes.len().div_ceil(CHUNK);
    let id = identifier();
    Ok(bytes.chunks(CHUNK).enumerate().map(|(part, data)| {
        json!({"type":"relay","to":to,"payload":Chunk { transfer:id.clone(),part,total,data:STANDARD.encode(data) }}).to_string()
    }).collect())
}
struct Assembly {
    started: Instant,
    total: usize,
    data: Vec<u8>,
    next: usize,
}
#[derive(Default)]
struct Decoder {
    transfers: HashMap<(String, String), Assembly>,
}
impl Decoder {
    fn push(&mut self, from: &str, payload: Value) -> Result<Option<Packet>> {
        self.transfers
            .retain(|_, a| a.started.elapsed() < Duration::from_secs(180));
        let c: Chunk = serde_json::from_value(payload)?;
        ensure!(
            c.transfer.len() == 32
                && c.transfer.bytes().all(|b| b.is_ascii_hexdigit())
                && (1..=MAX_PACKET.div_ceil(CHUNK)).contains(&c.total)
                && c.part < c.total
                && c.data.len() <= CHUNK * 2,
            "Invalid transfer fragment"
        );
        let bytes = STANDARD.decode(c.data)?;
        ensure!(bytes.len() <= CHUNK, "Fragment too large");
        let key = (from.to_string(), c.transfer);
        if c.part == 0 {
            ensure!(self.transfers.len() < 8, "Too many incoming transfers");
            self.transfers.insert(
                key.clone(),
                Assembly {
                    started: Instant::now(),
                    total: c.total,
                    data: vec![],
                    next: 0,
                },
            );
        }
        let buffered: usize = self.transfers.values().map(|a| a.data.len()).sum();
        ensure!(
            buffered + bytes.len() <= MAX_PACKET,
            "Incoming transfers too large"
        );
        let a = self
            .transfers
            .get_mut(&key)
            .context("Missing transfer fragment")?;
        ensure!(
            a.total == c.total && a.next == c.part,
            "Out-of-order transfer fragment"
        );
        a.next += 1;
        a.data.extend(bytes);
        if a.next != a.total {
            return Ok(None);
        }
        let a = self.transfers.remove(&key).unwrap();
        let mut raw = vec![];
        ZlibDecoder::new(a.data.as_slice())
            .take(MAX_PACKET as u64 + 1)
            .read_to_end(&mut raw)?;
        ensure!(raw.len() <= MAX_PACKET, "Expanded document is too large");
        Ok(Some(serde_json::from_slice(&raw)?))
    }
}
fn source(path: Option<PathBuf>) -> Result<Option<Source>> {
    path.map(|path| {
        ensure!(
            path.metadata()?.len() <= MAX_PDF as u64,
            "Shared PDFs must be at most 64 MiB"
        );
        let bytes = std::fs::read(path)?;
        ensure!(bytes.starts_with(b"%PDF-"), "Invalid PDF source");
        Ok(Source {
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            data: STANDARD.encode(bytes),
        })
    })
    .transpose()
}
fn receive_source(root: &Path, source: Source) -> Result<String> {
    ensure!(
        source.data.len() <= MAX_PDF.div_ceil(3) * 4,
        "PDF is too large"
    );
    let bytes = STANDARD.decode(source.data)?;
    ensure!(
        bytes.len() <= MAX_PDF && bytes.starts_with(b"%PDF-"),
        "Invalid shared PDF"
    );
    let hash = format!("{:x}", Sha256::digest(&bytes));
    ensure!(hash == source.sha256, "PDF checksum mismatch");
    let name = format!("files/exchange-{hash}.pdf");
    let temp = root.join("files").join(format!("{}.part", identifier()));
    std::fs::create_dir_all(root.join("files"))?;
    let result = (|| -> Result<()> {
        let mut f = std::fs::File::create(&temp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        std::fs::rename(&temp, root.join(&name))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result?;
    Ok(name)
}
fn publish(tx: &mpsc::SyncSender<Event>, event: Event) -> Result<()> {
    match tx.try_send(event) {
        Ok(()) => Ok(()),
        // Link::drop closes the UI receiver and asks the worker to drain its
        // outgoing edits. An incoming event racing with that drop must not
        // abort run() before the final queued stroke has been sent.
        Err(mpsc::TrySendError::Disconnected(_)) => Ok(()),
        Err(mpsc::TrySendError::Full(_)) => {
            bail!("Client is busy; reconnecting to synchronize")
        }
    }
}
fn worker(
    url: url::Url,
    mut code: Option<String>,
    root: PathBuf,
    rx: mpsc::Receiver<Command>,
    tx: mpsc::SyncSender<Event>,
    stop: Arc<AtomicBool>,
    restart: Arc<AtomicBool>,
) {
    let mut attempts = 0u32;
    while !stop.load(Ordering::Relaxed) {
        restart.store(false, Ordering::Relaxed);
        while rx.try_recv().is_ok() {} // changes are represented in next full state
        let result = run(&url, &mut code, &root, &rx, &tx, &stop, &restart);
        if stop.load(Ordering::Relaxed) {
            break;
        }
        match result {
            Ok(()) => break,
            Err(e) => {
                let _ = tx.try_send(Event::Status(format!("Reconnecting: {e}")));
            }
        }
        attempts = (attempts + 1).min(6);
        for _ in 0..attempts * 10 {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
}
fn run(
    url: &url::Url,
    code: &mut Option<String>,
    root: &Path,
    rx: &mpsc::Receiver<Command>,
    tx: &mpsc::SyncSender<Event>,
    stop: &AtomicBool,
    restart: &AtomicBool,
) -> Result<()> {
    let mut ws = socket(url)?;
    ws.send(Message::Text(
        match code {
            Some(c) => json!({"type":"join","version":1,"code":c}),
            None => json!({"type":"create","version":1}),
        }
        .to_string()
        .into(),
    ))?;
    let mut ready = false;
    let mut decoder = Decoder::default();
    let mut out = VecDeque::new();
    let mut last_send = Instant::now();
    let mut last_read = Instant::now();
    let mut closing: Option<Instant> = None;
    while !restart.load(Ordering::Relaxed) {
        if stop.load(Ordering::Relaxed) {
            let since = closing.get_or_insert_with(Instant::now);
            // Leaving a room drains completed edits briefly without blocking
            // the UI. Offline/large unfinished transfers remain in local files.
            if !ready || since.elapsed() >= Duration::from_secs(2) {
                break;
            }
        }
        if ready && out.is_empty() {
            if let Ok(cmd) = rx.try_recv() {
                let (packet, to) = match cmd {
                    Command::NeedState(peer) => (Packet::NeedState, Some(peer)),
                    Command::Snapshot {
                        to,
                        state,
                        source: path,
                    } => (
                        Packet::Snapshot {
                            state,
                            source: source(path)?,
                        },
                        to,
                    ),
                    Command::Ops { document, changes } => (Packet::Ops { document, changes }, None),
                };
                out = encode(packet, to)?;
            } else if closing.is_some() {
                break;
            }
        }
        if !out.is_empty() && last_send.elapsed() >= Duration::from_millis(20) {
            ws.send(Message::Text(out.pop_front().unwrap().into()))?;
            last_send = Instant::now();
        }
        match ws.read() {
            Ok(Message::Text(text)) => {
                last_read = Instant::now();
                if stop.load(Ordering::Relaxed) {
                    continue;
                }
                let v: Value = serde_json::from_str(&text)?;
                match v["type"].as_str() {
                    Some("welcome") => {
                        ensure!(v["version"] == 1, "Incompatible exchange server");
                        let new_code = room_code(v["code"].as_str().context("Missing room code")?)?;
                        *code = Some(new_code.clone());
                        ready = true;
                        publish(
                            tx,
                            Event::Connected {
                                code: new_code,
                                peers: serde_json::from_value(v["peers"].clone())?,
                            },
                        )?;
                    }
                    Some("peer_joined") => publish(
                        tx,
                        Event::Joined(v["peer"].as_str().context("Missing peer")?.into()),
                    )?,
                    Some("peer_left") => {
                        let peer = v["peer"].as_str().context("Missing peer")?;
                        decoder.transfers.retain(|(from, _), _| from != peer);
                        publish(tx, Event::Left(peer.into()))?;
                    }
                    Some("error") => {
                        let error = match v["code"].as_str() {
                            Some("room_not_found") => "Room has ended or code is incorrect. Start sharing again; local copies are kept.",
                            Some("room_full") => "This room is full (32 participants).",
                            _ => "Exchange server rejected the connection.",
                        };
                        publish(tx, Event::Ended(error.into()))?;
                        return Ok(());
                    }
                    Some("relay") => {
                        let from = v["from"].as_str().context("Missing sender")?;
                        if let Some(packet) = decoder.push(from, v["payload"].clone())? {
                            let event = match packet {
                                Packet::NeedState => Event::NeedState(from.into()),
                                Packet::Snapshot { state, source } => {
                                    state.validate()?;
                                    ensure!(
                                        (state.kind == "pdf") == source.is_some(),
                                        "Shared PDF source is missing or unexpected"
                                    );
                                    Event::Snapshot {
                                        state,
                                        source: source
                                            .map(|s| receive_source(root, s))
                                            .transpose()?,
                                    }
                                }
                                Packet::Ops { document, changes } => {
                                    Event::Ops { document, changes }
                                }
                            };
                            publish(tx, event)?;
                        }
                    }
                    _ => bail!("Unexpected exchange server message"),
                }
            }
            Ok(Message::Ping(_)) => {
                last_read = Instant::now();
                ws.flush()?;
            }
            Ok(Message::Pong(_)) => {
                last_read = Instant::now();
            }
            Ok(Message::Close(_)) => bail!("Connection closed"),
            Ok(_) => bail!("Unexpected binary message"),
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e.into()),
        }
        ensure!(
            last_read.elapsed() < Duration::from_secs(70),
            "Server stopped responding"
        );
    }
    let _ = ws.close(None);
    if restart.load(Ordering::Relaxed) {
        bail!("Synchronizing after connection congestion");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_ui_receiver_allows_outgoing_drain_but_congestion_reconnects() {
        let (tx, rx) = mpsc::sync_channel(1);
        publish(&tx, Event::Joined("first".into())).unwrap();
        assert!(publish(&tx, Event::Joined("second".into())).is_err());
        drop(rx);
        publish(&tx, Event::Joined("arrived_during_close".into())).unwrap();
    }
    #[test]
    fn chunk_transfer_roundtrip_and_sender_isolation() {
        // Poorly compressible, deterministic data forces several fragments.
        let data = (0..12000).map(|_| identifier()).collect::<String>();
        let packet = Packet::Snapshot {
            state: SharedState {
                document: identifier(),
                kind: "book".into(),
                cells: Default::default(),
            },
            source: Some(Source {
                sha256: "test".into(),
                data: data.clone(),
            }),
        };
        let frames = encode(packet, None).unwrap();
        assert!(frames.len() > 2);
        let mut decoder = Decoder::default();
        let mut decoded = None;
        for (index, frame) in frames.iter().enumerate() {
            let value: Value = serde_json::from_str(frame).unwrap();
            if index == 1 {
                assert!(decoder
                    .push("wrong-peer", value["payload"].clone())
                    .is_err());
            }
            decoded = decoder.push("sender", value["payload"].clone()).unwrap();
        }
        let Some(Packet::Snapshot {
            source: Some(source),
            ..
        }) = decoded
        else {
            panic!("Missing snapshot");
        };
        assert_eq!(source.data, data);
        assert!(decoder.transfers.is_empty());
    }
    #[test]
    fn pdf_transfer_checks_content_and_hash_before_writing() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = b"%PDF-1.4\nclient owned test file\n%%EOF";
        let hash = format!("{:x}", Sha256::digest(bytes));
        assert!(receive_source(
            dir.path(),
            Source {
                sha256: "incorrect".into(),
                data: STANDARD.encode(bytes)
            }
        )
        .is_err());
        assert!(!dir.path().join("files").exists());
        let name = receive_source(
            dir.path(),
            Source {
                sha256: hash,
                data: STANDARD.encode(bytes),
            },
        )
        .unwrap();
        assert_eq!(std::fs::read(dir.path().join(name)).unwrap(), bytes);
        assert_eq!(
            std::fs::read_dir(dir.path().join("files")).unwrap().count(),
            1
        );
    }
    #[test]
    fn rejects_oversized_and_out_of_order_transfers() {
        let mut decoder = Decoder::default();
        assert!(decoder
            .push(
                "peer",
                json!({"transfer":identifier(),"part":0,"total":usize::MAX,"data":""})
            )
            .is_err());
        let id = identifier();
        assert!(decoder
            .push(
                "peer",
                json!({"transfer":id,"part":0,"total":3,"data":STANDARD.encode([0;4])})
            )
            .unwrap()
            .is_none());
        assert!(decoder
            .push("peer", json!({"transfer":id,"part":2,"total":3,"data":""}))
            .is_err());
    }
}
