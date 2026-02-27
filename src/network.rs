use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Vec3Net {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3Net {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    pub fn to_cgmath(&self) -> cgmath::Vector3<f32> {
        cgmath::Vector3::new(self.x, self.y, self.z)
    }
}

impl From<cgmath::Vector3<f32>> for Vec3Net {
    fn from(v: cgmath::Vector3<f32>) -> Self {
        Self::new(v.x, v.y, v.z)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlayerState {
    pub id: u32,
    pub position: Vec3Net,
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EntityState {
    pub net_id:     u32,
    pub position:   Vec3Net,
    pub yaw:        f32,
    pub model_name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", content = "data")]
pub enum ClientMessage {
    PlayerInput {
        forward:  f32,
        backward: f32,
        left:     f32,
        right:    f32,
        jump:     bool,
        yaw:      f32,
        pitch:    f32,
        position: Vec3Net,
    },
    BreakBlock { x: i32, y: i32, z: i32 },
    PlaceBlock { x: i32, y: i32, z: i32, block_type: u32 },
    RequestChunk { cx: i32, cz: i32 },
    Chat { message: String },
    PickupDrop { drop_id: u32 },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", content = "data")]
pub enum ServerMessage {
    Welcome {
        player_id: u32,
        spawn:     Vec3Net,
    },
    PlayerStates(Vec<PlayerState>),
    EntityStates(Vec<EntityState>),
    BlockUpdate { x: i32, y: i32, z: i32, block_type: u32 },
    ChunkData { cx: i32, cz: i32, blocks: Vec<u32> },
    AvailableChunks(Vec<[i32; 2]>),
    PlayerLeft { player_id: u32 },
    Chat { sender_id: u32, message: String },
    SpawnDrop { drop_id: u32, x: f32, y: f32, z: f32, item_id: u32 },
    DespawnDrop { drop_id: u32 },
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use super::*;

    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use tokio::{
        runtime::Runtime,
        sync::{broadcast, mpsc as tokio_mpsc},
    };
    use tokio_tungstenite::{
        accept_async, connect_async,
        tungstenite::Message,
    };
    use futures_util::{SinkExt, StreamExt};

    pub struct ClientEvent {
        pub player_id: u32,
        pub message:   ClientMessage,
    }

    pub struct NetServer {
        pub lan_address: String,

        broadcast_tx: broadcast::Sender<String>,

        unicast_txs: Arc<Mutex<HashMap<u32, tokio_mpsc::UnboundedSender<String>>>>,

        pub incoming_rx: std::sync::mpsc::Receiver<ClientEvent>,

        pub player_states: Arc<Mutex<HashMap<u32, PlayerState>>>,

        _rt: Runtime,
    }

    impl NetServer {
        pub fn start(port: u16) -> anyhow::Result<Self> {
            let rt = Runtime::new()?;

            let lan_ip = local_ip_address::local_ip()
                .map(|ip| ip.to_string())
                .unwrap_or_else(|_| "127.0.0.1".to_string());
            let bind_addr  = format!("0.0.0.0:{}", port);
            let lan_address = format!("ws://{}:{}", lan_ip, port);

            let (broadcast_tx, _) = broadcast::channel::<String>(512);
            let unicast_txs: Arc<Mutex<HashMap<u32, tokio_mpsc::UnboundedSender<String>>>> =
                Arc::new(Mutex::new(HashMap::new()));
            let player_states: Arc<Mutex<HashMap<u32, PlayerState>>> =
                Arc::new(Mutex::new(HashMap::new()));
            let next_id = Arc::new(Mutex::new(1u32));

            let (game_tx, game_rx) = std::sync::mpsc::channel::<ClientEvent>();

            let broadcast_tx_clone  = broadcast_tx.clone();
            let unicast_txs_clone   = unicast_txs.clone();
            let player_states_clone = player_states.clone();

            rt.spawn(async move {
                let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
                    Ok(l) => l,
                    Err(e) => {
                        log::error!("Server: failed to bind {}: {}", bind_addr, e);
                        return;
                    }
                };
                log::info!("Server listening on {}", bind_addr);

                while let Ok((stream, addr)) = listener.accept().await {
                    log::info!("New connection from {}", addr);

                    let player_id = {
                        let mut id = next_id.lock().unwrap();
                        let pid = *id;
                        *id += 1;
                        pid
                    };

                    let broadcast_tx2  = broadcast_tx_clone.clone();
                    let unicast_txs2   = unicast_txs_clone.clone();
                    let player_states2 = player_states_clone.clone();
                    let game_tx2       = game_tx.clone();

                    tokio::spawn(async move {
                        Self::handle_client(
                            stream, player_id,
                            broadcast_tx2, unicast_txs2, player_states2, game_tx2,
                        ).await;
                    });
                }
            });

            Ok(Self {
                lan_address,
                broadcast_tx,
                unicast_txs,
                incoming_rx: game_rx,
                player_states,
                _rt: rt,
            })
        }

        async fn handle_client(
            stream:        tokio::net::TcpStream,
            player_id:     u32,
            broadcast_tx:  broadcast::Sender<String>,
            unicast_txs:   Arc<Mutex<HashMap<u32, tokio_mpsc::UnboundedSender<String>>>>,
            player_states: Arc<Mutex<HashMap<u32, PlayerState>>>,
            game_tx:       std::sync::mpsc::Sender<ClientEvent>,
        ) {
            let ws = match accept_async(stream).await {
                Ok(ws) => ws,
                Err(e) => {
                    log::error!("WS handshake error for player {}: {}", player_id, e);
                    return;
                }
            };

            let (unicast_tx, mut unicast_rx) = tokio_mpsc::unbounded_channel::<String>();
            unicast_txs.lock().unwrap().insert(player_id, unicast_tx);

            let spawn_pos = Vec3Net::new(0.0, 100.0, 0.0);

            {
                let mut states = player_states.lock().unwrap();
                states.insert(player_id, PlayerState {
                    id:       player_id,
                    position: spawn_pos.clone(),
                    yaw:      0.0,
                    pitch:    0.0,
                });
            }

            let (mut ws_write, mut ws_read) = ws.split();

            let welcome = serde_json::to_string(&ServerMessage::Welcome {
                player_id,
                spawn: spawn_pos,
            }).unwrap();
            if ws_write.send(Message::Text(welcome.into())).await.is_err() {
                return;
            }

            let existing: Vec<PlayerState> = {
                let states = player_states.lock().unwrap();
                states.values()
                    .filter(|s| s.id != player_id)
                    .cloned()
                    .collect()
            };
            if !existing.is_empty() {
                if let Ok(json) = serde_json::to_string(&ServerMessage::PlayerStates(existing)) {
                    let _ = ws_write.send(Message::Text(json.into())).await;
                }
            }

            let mut bcast_rx = broadcast_tx.subscribe();

            loop {
                tokio::select! {
                    msg = ws_read.next() => {
                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                match serde_json::from_str::<ClientMessage>(&text) {
                                    Ok(client_msg) => {
                                        if let ClientMessage::PlayerInput {
                                            yaw, pitch, position, ..
                                        } = &client_msg {
                                            if let Some(state) = player_states.lock().unwrap().get_mut(&player_id) {
                                                state.yaw      = *yaw;
                                                state.pitch    = *pitch;
                                                state.position = position.clone();
                                            }
                                        }
                                        let _ = game_tx.send(ClientEvent {
                                            player_id,
                                            message: client_msg,
                                        });
                                    }
                                    Err(e) => log::warn!("bad client msg: {}", e),
                                }
                            }
                            Some(Ok(Message::Close(_))) | None => break,
                            _ => {}
                        }
                    }
                    out = unicast_rx.recv() => {
                        match out {
                            Some(json) => {
                                if ws_write.send(Message::Text(json.into())).await.is_err() {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                    bcast = bcast_rx.recv() => {
                        match bcast {
                            Ok(json) => {
                                if ws_write.send(Message::Text(json.into())).await.is_err() {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                log::warn!("client {} lagged by {} msgs", player_id, n);
                            }
                            Err(_) => break,
                        }
                    }
                }
            }

            log::info!("Player {} disconnected", player_id);
            unicast_txs.lock().unwrap().remove(&player_id);
            player_states.lock().unwrap().remove(&player_id);

            let left_msg = serde_json::to_string(&ServerMessage::PlayerLeft { player_id }).unwrap();
            let _ = broadcast_tx.send(left_msg);
        }

        pub fn broadcast(&self, msg: &ServerMessage) {
            if let Ok(json) = serde_json::to_string(msg) {
                let _ = self.broadcast_tx.send(json);
            }
        }

        pub fn send_to_client(&self, player_id: u32, msg: &ServerMessage) {
            if let Ok(json) = serde_json::to_string(msg) {
                if let Some(tx) = self.unicast_txs.lock().unwrap().get(&player_id) {
                    let _ = tx.send(json);
                }
            }
        }

        pub fn poll_incoming(&self) -> Vec<ClientEvent> {
            let mut events = Vec::new();
            while let Ok(ev) = self.incoming_rx.try_recv() {
                events.push(ev);
            }
            events
        }

        pub fn player_states_snapshot(&self) -> Vec<PlayerState> {
            self.player_states
                .lock()
                .unwrap()
                .values()
                .cloned()
                .collect()
        }
    }

    pub struct NetClient {
        rx: std::sync::mpsc::Receiver<ServerMessage>,
        tx: std::sync::mpsc::SyncSender<ClientMessage>,
        pub my_id: Option<u32>,
        _rt: Runtime,
    }

    impl NetClient {
        pub fn connect(url: &str) -> anyhow::Result<Self> {
            let rt = Runtime::new()?;

            let url_owned = url.to_string();

            let (srv_tx, srv_rx) = std::sync::mpsc::sync_channel::<ServerMessage>(1024);
            let (cli_tx, cli_rx) = tokio_mpsc::unbounded_channel::<ClientMessage>();
            let cli_sync_tx = {
                let (std_tx, std_rx) = std::sync::mpsc::sync_channel::<ClientMessage>(256);
                let cli_tx_clone = cli_tx.clone();

                std::thread::spawn(move || {
                    while let Ok(msg) = std_rx.recv() {
                        if cli_tx_clone.send(msg).is_err() {
                            break;
                        }
                    }
                });
                std_tx
            };

            rt.spawn(async move {
                let url_parsed = match tokio_tungstenite::tungstenite::http::Uri::builder()
                    .path_and_query(url_owned.trim_start_matches("ws:"))
                    .build()
                {
                    Ok(u) => u,
                    Err(_) => {
                        match connect_async(&url_owned).await {
                            Ok((ws_stream, _)) => {
                                Self::run_stream(ws_stream, srv_tx, cli_rx).await;
                                return;
                            }
                            Err(e) => {
                                log::error!("Client connect failed: {}", e);
                                return;
                            }
                        }
                    }
                };
                match connect_async(&url_owned).await {
                    Ok((ws_stream, _)) => {
                        Self::run_stream(ws_stream, srv_tx, cli_rx).await;
                    }
                    Err(e) => log::error!("Client connect failed: {}", e),
                }
                let _ = url_parsed;
            });

            Ok(Self {
                rx: srv_rx,
                tx: cli_sync_tx,
                my_id: None,
                _rt: rt,
            })
        }

        async fn run_stream<S>(
            ws: tokio_tungstenite::WebSocketStream<S>,
            srv_tx: std::sync::mpsc::SyncSender<ServerMessage>,
            mut cli_rx: tokio_mpsc::UnboundedReceiver<ClientMessage>,
        )
        where
            S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
        {
            let (mut write, mut read) = ws.split();

            loop {
                tokio::select! {
                    msg = read.next() => {
                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                match serde_json::from_str::<ServerMessage>(&text) {
                                    Ok(m) => { let _ = srv_tx.send(m); }
                                    Err(e) => log::warn!("bad server msg: {}", e),
                                }
                            }
                            Some(Ok(Message::Close(_))) | None => break,
                            _ => {}
                        }
                    }
                    out = cli_rx.recv() => {
                        match out {
                            Some(m) => {
                                if let Ok(json) = serde_json::to_string(&m) {
                                    if write.send(Message::Text(json.into())).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
            log::info!("Client disconnected from server");
        }

        pub fn poll(&mut self) -> Vec<ServerMessage> {
            let mut messages = Vec::new();
            while let Ok(msg) = self.rx.try_recv() {
                if let ServerMessage::Welcome { player_id, .. } = &msg {
                    self.my_id = Some(*player_id);
                }
                messages.push(msg);
            }
            messages
        }

        pub fn send(&self, msg: &ClientMessage) {
            let _ = self.tx.try_send(msg.clone());
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use super::*;
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};
    use wasm_bindgen::{prelude::*, JsCast};
    use web_sys::{ErrorEvent, MessageEvent, WebSocket};

    pub struct NetClient {
        ws: WebSocket,
        incoming: Rc<RefCell<VecDeque<ServerMessage>>>,
        pub my_id: Option<u32>,
        _on_message: Closure<dyn FnMut(MessageEvent)>,
        _on_error:   Closure<dyn FnMut(ErrorEvent)>,
        _on_close:   Closure<dyn FnMut(web_sys::CloseEvent)>,
    }

    impl NetClient {
        pub fn connect(url: &str) -> anyhow::Result<Self> {
            let ws = WebSocket::new(url)
                .map_err(|e| anyhow::anyhow!("WS open failed: {:?}", e))?;
            ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

            let queue: Rc<RefCell<VecDeque<ServerMessage>>> =
                Rc::new(RefCell::new(VecDeque::new()));

            let q_clone = queue.clone();
            let on_message = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
                if let Some(text) = e.data().as_string() {
                    match serde_json::from_str::<ServerMessage>(&text) {
                        Ok(msg) => q_clone.borrow_mut().push_back(msg),
                        Err(err) => log::warn!("bad server msg: {}", err),
                    }
                }
            });
            ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

            let on_error = Closure::<dyn FnMut(_)>::new(|e: ErrorEvent| {
                log::error!("WS error: {:?}", e.message());
            });
            ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

            let on_close = Closure::<dyn FnMut(_)>::new(|_: web_sys::CloseEvent| {
                log::info!("WS closed");
            });
            ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

            Ok(Self {
                ws,
                incoming: queue,
                my_id: None,
                _on_message: on_message,
                _on_error:   on_error,
                _on_close:   on_close,
            })
        }

        pub fn poll(&mut self) -> Vec<ServerMessage> {
            let mut messages: Vec<ServerMessage> = self.incoming.borrow_mut().drain(..).collect();
            for msg in &messages {
                if let ServerMessage::Welcome { player_id, .. } = msg {
                    self.my_id = Some(*player_id);
                }
            }
            messages
        }

        pub fn send(&self, msg: &ClientMessage) {
            if let Ok(json) = serde_json::to_string(msg) {
                if let Err(e) = self.ws.send_with_str(&json) {
                    log::warn!("WS send error: {:?}", e);
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{ClientEvent, NetClient, NetServer};

#[cfg(target_arch = "wasm32")]
pub use wasm::NetClient;

pub fn serialize_chunk_blocks(
    chunk: &crate::chunk::Chunk,
) -> Vec<u32> {
    use crate::chunk::{CHUNK_X_SIZE, CHUNK_Y_SIZE, CHUNK_Z_SIZE, block_index};
    let mut flat = Vec::new();
    for x in 0..CHUNK_X_SIZE {
        for y in 0..CHUNK_Y_SIZE {
            for z in 0..CHUNK_Z_SIZE {
                let mat = chunk.block_types[block_index(x, y, z)];
                if mat != 0 {
                    flat.push(x as u32);
                    flat.push(y as u32);
                    flat.push(z as u32);
                    flat.push(mat);
                }
            }
        }
    }
    flat
}
