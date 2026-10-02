//! HTTP routes, the `/room/CODE` WebSocket upgrade, connection limits and shutdown.

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Incoming as RequestBody;
use hyper::header::{self, HeaderMap, HeaderValue};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use serde::Serialize;
use tokio::net::{TcpListener, ToSocketAddrs};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::config::BuildInfo;
use crate::dashboard::{Dashboard, PAGE, PAGE_HEADERS};
use crate::host::HostFactory;
use crate::monitor::{
    BusyProbe, LagRecorder, LiveReading, MonitorInput, READING_INTERVAL_MS, ServerMonitor,
    ServerStats, WallClock,
};
use crate::protocol::{CONTENT_VERSION, PROTOCOL_VERSION, SERVER_BUILD, is_room_code};
use crate::rate_limit::RateLimit;
use crate::room_catalog::RoomCatalog;
use crate::room_task::{
    Admission, Registry, RoomCommand, RoomHandle, ServerRoomEvents, spawn_room,
};
use crate::session::{Clock, MAX_PENDING_CONNECTIONS, RoomSample, RoomSocket};
use crate::socket::{self, Ending, RoomLink, SocketHandle};
use crate::websocket::{self, Codec, Role, extension};
use crate::wire::{ConnectionBytes, CountingIo, WireTotals};

/// Live rooms per process. A busy room measured about 3–4 ms of each 50 ms tick on the
/// one-vCPU VPS, so ten busy rooms use roughly two thirds of it, leaving headroom for
/// bursts. Raise it with `MAX_ROOMS` on a larger host. Joining an existing room is never
/// refused by this cap.
pub const DEFAULT_MAX_ROOMS: usize = 10;
/// Concurrent sockets from one IP; covers a 32-bot traffic swarm sharing an egress address.
pub const DEFAULT_MAX_SOCKETS_PER_IP: usize = 32;
/// Headroom over `MAX_CLIENT_MESSAGE_BYTES`; the session enforces the exact protocol limit.
pub const MAX_FRAME_BYTES: usize = 8192;
/// Time given to clients to finish closing handshakes when the server stops.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(1);
/// Slow request headers are cut off, like Node's `headersTimeout`.
const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(30);
/// A room that does not answer a monitor sample by then is left out of the reading.
const SAMPLE_TIMEOUT: Duration = Duration::from_millis(500);
/// The runtime lag probe's interval.
const LAG_PROBE_INTERVAL: Duration = Duration::from_millis(10);
const LOCAL_ADDRESSES: [&str; 3] = ["127.0.0.1", "::1", "::ffff:127.0.0.1"];

pub type Log = Arc<dyn Fn(&str) + Send + Sync>;
type Body = BoxBody<Bytes, Infallible>;

pub struct ServerOptions {
    pub allowed_origins: Vec<String>,
    /// Take the client IP from the last `X-Forwarded-For` hop (the local reverse proxy).
    pub trust_proxy: bool,
    pub max_rooms: usize,
    pub max_sockets_per_ip: usize,
    /// Room lifecycle and summary lines; the binary prints them (the systemd journal).
    pub log: Log,
    /// The commit and build time `/health` reports beside the build stamps.
    pub build: BuildInfo,
}

impl ServerOptions {
    pub fn new(allowed_origins: Vec<String>, trust_proxy: bool) -> Self {
        Self {
            allowed_origins,
            trust_proxy,
            max_rooms: DEFAULT_MAX_ROOMS,
            max_sockets_per_ip: DEFAULT_MAX_SOCKETS_PER_IP,
            log: Arc::new(|line| println!("{line}")),
            build: BuildInfo::default(),
        }
    }
}

struct Limits {
    connection: RateLimit,
    entry: RateLimit,
    directory: RateLimit,
    dashboard: RateLimit,
}

type SpawnRoom = Box<dyn Fn(&str, u64) -> RoomHandle + Send + Sync>;

struct Shared {
    options: ServerOptions,
    clock: Clock,
    registry: Arc<Registry>,
    next_instance: AtomicU64,
    spawn_room: SpawnRoom,
    catalog: Arc<Mutex<RoomCatalog>>,
    limits: Mutex<Limits>,
    sockets_by_ip: Mutex<HashMap<String, usize>>,
    open_sockets: AtomicUsize,
    wire: Arc<WireTotals>,
    monitor: Arc<Mutex<ServerMonitor>>,
    dashboard: Mutex<Dashboard>,
    stopping: AtomicBool,
    terminate: watch::Sender<bool>,
}

/// The multiplayer host: `/health` (and `/health/`), `/rooms`, the `/room/CODE` WebSocket, the public
/// `/dashboard` and the loopback-only `/stats`.
pub struct MultiplayerServer {
    shared: Arc<Shared>,
    address: SocketAddr,
    tasks: Vec<JoinHandle<()>>,
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

fn busy_probe() -> BusyProbe {
    let metrics = tokio::runtime::Handle::current().metrics();
    Box::new(move || {
        let workers = metrics.num_workers();
        let busy = (0..workers)
            .map(|worker| metrics.worker_total_busy_duration(worker))
            .sum();
        (busy, workers)
    })
}

impl MultiplayerServer {
    /// Binds the listener and starts serving, the monitor and the lag probe.
    pub async fn listen<F: HostFactory>(
        options: ServerOptions,
        factory: F,
        address: impl ToSocketAddrs,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind(address).await?;
        let address = listener.local_addr()?;
        let epoch = Instant::now();
        let clock: Clock = Arc::new(move || epoch.elapsed().as_millis() as u64);
        let wall: WallClock = Arc::new(unix_ms);
        let lag = Arc::new(LagRecorder::default());
        let log = options.log.clone();
        let mut monitor = ServerMonitor::new(wall, Box::new(move |line| log(line)))
            .with_runtime(lag.clone(), busy_probe());
        monitor.start();
        let monitor = Arc::new(Mutex::new(monitor));
        let catalog = Arc::new(Mutex::new(RoomCatalog::default()));
        let registry: Arc<Registry> = Arc::default();
        let factory = Arc::new(factory);
        let spawn_room: SpawnRoom = {
            let (clock, catalog, monitor, registry) = (
                clock.clone(),
                catalog.clone(),
                monitor.clone(),
                registry.clone(),
            );
            Box::new(move |code, instance| {
                let events = ServerRoomEvents {
                    room: code.to_string(),
                    catalog: catalog.clone(),
                    monitor: monitor.clone(),
                    clock: clock.clone(),
                };
                spawn_room(
                    code.to_string(),
                    instance,
                    factory.clone(),
                    clock.clone(),
                    epoch,
                    events,
                    registry.clone(),
                )
            })
        };
        let (terminate, _) = watch::channel(false);
        let max_rooms = options.max_rooms;
        let shared = Arc::new(Shared {
            options,
            clock,
            registry,
            next_instance: AtomicU64::new(1),
            spawn_room,
            catalog,
            limits: Mutex::new(Limits {
                connection: RateLimit::per_minute(60),
                entry: RateLimit::per_minute(120),
                directory: RateLimit::per_minute(120),
                dashboard: RateLimit::per_minute(30),
            }),
            sockets_by_ip: Mutex::default(),
            open_sockets: AtomicUsize::new(0),
            wire: Arc::default(),
            monitor,
            dashboard: Mutex::new(Dashboard::new(max_rooms)),
            stopping: AtomicBool::new(false),
            terminate,
        });
        let tasks = vec![
            tokio::spawn(accept_loop(shared.clone(), listener)),
            tokio::spawn(monitor_loop(shared.clone())),
            tokio::spawn(lag_probe(lag)),
        ];
        Ok(Self {
            shared,
            address,
            tasks,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }

    /// Live room codes, oldest room first.
    pub fn room_codes(&self) -> Vec<String> {
        let rooms = self.shared.registry.lock().expect("room registry");
        let mut codes: Vec<(u64, String)> = rooms
            .iter()
            .map(|(code, room)| (room.instance, code.clone()))
            .collect();
        codes.sort();
        codes.into_iter().map(|(_, code)| code).collect()
    }

    /// Takes a reading now (and shows it on the dashboard), as the one-second timer does.
    pub async fn read_now(&self) -> LiveReading {
        let input = gather(&self.shared).await;
        let mut monitor = self.shared.monitor.lock().expect("monitor");
        let reading = monitor.read(input);
        self.shared
            .dashboard
            .lock()
            .expect("dashboard")
            .broadcast(&reading, &monitor);
        reading
    }

    /// Takes a `/stats` sample now, as every tenth timer reading does.
    pub async fn sample_now(&self) -> ServerStats {
        sample(&self.shared).await
    }

    /// Resets every room (`room-reset`, close code 1012), gives clients a second to finish
    /// closing, then cuts every remaining connection.
    pub async fn close(self) {
        let shared = &self.shared;
        shared.stopping.store(true, Ordering::Relaxed);
        shared.dashboard.lock().expect("dashboard").close();
        // Stop accepting, sampling and probing first.
        for task in &self.tasks {
            task.abort();
        }
        let rooms: Vec<RoomHandle> = shared
            .registry
            .lock()
            .expect("room registry")
            .values()
            .cloned()
            .collect();
        let mut pending = Vec::new();
        for room in rooms {
            let (done, finished) = oneshot::channel();
            let reset = RoomCommand::Reset {
                reason: "server-restart".into(),
                done,
            };
            if room.sender.send(reset).await.is_ok() {
                pending.push(finished);
            }
        }
        for finished in pending {
            let _ = finished.await;
        }
        let deadline = Instant::now() + SHUTDOWN_GRACE;
        while shared.open_sockets.load(Ordering::Relaxed) > 0 && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        shared.terminate.send_replace(true);
    }
}

async fn accept_loop(shared: Arc<Shared>, listener: TcpListener) {
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                // Out of file descriptors and similar: back off instead of spinning.
                eprintln!("accept failed: {error}");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let _ = stream.set_nodelay(true);
        tokio::spawn(serve_connection(shared.clone(), stream, peer));
    }
}

async fn serve_connection(shared: Arc<Shared>, stream: tokio::net::TcpStream, peer: SocketAddr) {
    let bytes = ConnectionBytes::new(shared.wire.clone());
    let io = TokioIo::new(CountingIo::new(stream, bytes.clone()));
    let service = {
        let shared = shared.clone();
        service_fn(move |request| {
            let (shared, bytes) = (shared.clone(), bytes.clone());
            async move { Ok::<_, Infallible>(route(&shared, request, peer, bytes).await) }
        })
    };
    let connection = http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_READ_TIMEOUT)
        .serve_connection(io, service)
        .with_upgrades();
    let mut terminate = shared.terminate.subscribe();
    tokio::select! {
        _ = connection => {}
        () = socket::stopped(&mut terminate) => {}
    }
}

fn text_body(text: impl Into<Bytes>) -> Body {
    Full::new(text.into()).boxed()
}

fn empty_body() -> Body {
    Empty::new().boxed()
}

enum Reply {
    None,
    Text(&'static str),
    Json(String),
}

/// A response like the TypeScript `reply`: extra headers, then a content type for a body.
fn reply(status: u16, body: Reply, headers: &[(&str, &str)]) -> Response<Body> {
    let mut response = Response::builder().status(status);
    for (name, value) in headers {
        response = response.header(*name, *value);
    }
    let (content_type, body) = match body {
        Reply::None => (None, empty_body()),
        Reply::Text(text) => (Some("text/plain"), text_body(text)),
        Reply::Json(json) => (Some("application/json"), text_body(json)),
    };
    if let Some(content_type) = content_type {
        response = response.header(header::CONTENT_TYPE, content_type);
    }
    response.body(body).expect("valid response")
}

fn header_text<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
}

fn allowed(shared: &Shared, headers: &HeaderMap) -> bool {
    let origin = header_text(headers, "origin");
    shared
        .options
        .allowed_origins
        .iter()
        .any(|allowed| allowed == origin)
}

fn client_ip(shared: &Shared, headers: &HeaderMap, peer: SocketAddr) -> String {
    if shared.options.trust_proxy {
        let forwarded: Vec<&str> = headers
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|value| value.to_str().ok())
            .collect();
        let joined = forwarded.join(",");
        if let Some(last) = joined
            .rsplit(',')
            .next()
            .map(str::trim)
            .filter(|hop| !hop.is_empty())
        {
            return last.to_string();
        }
    }
    peer.ip().to_string()
}

fn is_upgrade(headers: &HeaderMap) -> bool {
    headers.contains_key(header::UPGRADE)
        && headers
            .get_all(header::CONNECTION)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(','))
            .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
}

/// The code in `/room/CODE`, or `None` for any other path.
fn room_path(path: &str) -> Option<&str> {
    path.strip_prefix("/room/")
        .filter(|code| !code.is_empty() && !code.contains('/'))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Health<'a> {
    version: u32,
    content_version: &'static str,
    server_build: &'static str,
    // The same fields as the page's `/health`, when the image recorded them.
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dirty: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    built_at: Option<&'a str>,
}

async fn route(
    shared: &Arc<Shared>,
    mut request: Request<RequestBody>,
    peer: SocketAddr,
    bytes: Arc<ConnectionBytes>,
) -> Response<Body> {
    if is_upgrade(request.headers()) {
        return upgrade(shared, &mut request, peer, bytes);
    }
    let path = request.uri().path().to_string();
    let headers = request.headers();
    let origin = header_text(headers, "origin").to_string();
    let cors: [(&str, &str); 3] = [
        ("Access-Control-Allow-Origin", &origin),
        ("Cache-Control", "no-store"),
        ("Vary", "Origin"),
    ];
    match path.as_str() {
        // `/health/` too, matching the static page's `/health/` that GitHub Pages redirects to.
        "/health" | "/health/" => {
            // Pretty-printed because operators read it in a browser; /rooms stays compact.
            let build = &shared.options.build;
            let health = Health {
                version: PROTOCOL_VERSION,
                content_version: CONTENT_VERSION,
                server_build: SERVER_BUILD,
                commit: build.commit.as_deref(),
                dirty: build.commit.as_ref().map(|_| build.dirty),
                built_at: build.built_at.as_deref(),
            };
            let body = serde_json::to_string_pretty(&health).expect("health serializes") + "\n";
            reply(200, Reply::Json(body), &[])
        }
        "/stats" => {
            // Operator view with every room code, including unlisted ones: only a direct
            // local request qualifies. Caddy also refuses the path, and proxied requests
            // carry X-Forwarded-For.
            let local = LOCAL_ADDRESSES.contains(&peer.ip().to_string().as_str());
            if !local || headers.contains_key("x-forwarded-for") {
                return reply(404, Reply::Text("Not found"), &[]);
            }
            let latest = shared.monitor.lock().expect("monitor").latest.clone();
            let stats = match latest {
                Some(stats) => stats,
                None => sample(shared).await,
            };
            let json = serde_json::to_string(&stats).expect("stats serialize");
            reply(200, Reply::Json(json), &[("Cache-Control", "no-store")])
        }
        // The bare address shows the dashboard: people open it in a browser, and scripts
        // read /health.
        "/" | "/dashboard" | "/dashboard/" => {
            let mut response = Response::builder().status(200);
            for (name, value) in PAGE_HEADERS {
                response = response.header(name, value);
            }
            response.body(text_body(PAGE)).expect("valid response")
        }
        "/dashboard/stream" => {
            let ip = client_ip(shared, headers, peer);
            let now = (shared.clock)();
            if !shared
                .limits
                .lock()
                .expect("limits")
                .dashboard
                .allow(&ip, now)
            {
                return reply(
                    429,
                    Reply::Text("Too many dashboard connections; try again shortly"),
                    &[],
                );
            }
            let stream = {
                let monitor = shared.monitor.lock().expect("monitor");
                shared.dashboard.lock().expect("dashboard").stream(&monitor)
            };
            match stream {
                None => reply(503, Reply::Text("Too many dashboard viewers"), &[]),
                Some(stream) => Response::builder()
                    .status(200)
                    .header(header::CONTENT_TYPE, "text/event-stream")
                    .header(header::CACHE_CONTROL, "no-store")
                    .body(stream.boxed())
                    .expect("valid response"),
            }
        }
        "/rooms" => {
            if !allowed(shared, headers) {
                return reply(403, Reply::Text("Origin not allowed"), &[]);
            }
            if request.method() != Method::GET {
                return reply(405, Reply::None, &cors);
            }
            let ip = client_ip(shared, headers, peer);
            let now = (shared.clock)();
            if !shared
                .limits
                .lock()
                .expect("limits")
                .directory
                .allow(&ip, now)
            {
                return reply(
                    429,
                    Reply::Text("Too many refreshes; try again shortly"),
                    &cors,
                );
            }
            // Battle Setup asks for every room. Plain requests, such as the traffic bots',
            // see only rooms on standard maps.
            let extra_levels = request
                .uri()
                .query()
                .unwrap_or("")
                .split('&')
                .any(|pair| pair.split('=').next() == Some("extralevels"));
            let rooms = shared
                .catalog
                .lock()
                .expect("catalog")
                .list(now, extra_levels);
            let json = serde_json::to_string(&serde_json::json!({ "rooms": rooms }))
                .expect("rooms serialize");
            reply(200, Reply::Json(json), &cors)
        }
        _ => match room_path(&path) {
            Some(code) if is_room_code(code) => {
                if !allowed(shared, headers) {
                    return reply(403, Reply::Text("Origin not allowed"), &[]);
                }
                reply(426, Reply::Text("WebSocket required"), &[])
            }
            _ => reply(404, Reply::Text("Not found"), &[]),
        },
    }
}

/// A refused upgrade, written like the TypeScript's raw response: the reason phrase is
/// the message and the connection closes.
fn refuse(status: u16, text: &'static str) -> Response<Body> {
    let mut response = reply(status, Reply::Text(text), &[("Connection", "close")]);
    response
        .extensions_mut()
        .insert(hyper::ext::ReasonPhrase::from_static(text.as_bytes()));
    response
}

/// A handshake `ws` itself rejects (after the server's own checks passed).
fn abort_handshake(
    status: StatusCode,
    text: &'static str,
    extra: Option<(&str, &str)>,
) -> Response<Body> {
    let mut response = Response::builder()
        .status(status)
        .header(header::CONNECTION, "close")
        .header(header::CONTENT_TYPE, "text/html");
    if let Some((name, value)) = extra {
        response = response.header(name, value);
    }
    response.body(text_body(text)).expect("valid response")
}

fn upgrade(
    shared: &Arc<Shared>,
    request: &mut Request<RequestBody>,
    peer: SocketAddr,
    bytes: Arc<ConnectionBytes>,
) -> Response<Body> {
    let headers = request.headers();
    let Some(code) = room_path(request.uri().path())
        .filter(|code| is_room_code(code))
        .map(str::to_string)
    else {
        return refuse(404, "Not Found");
    };
    if !allowed(shared, headers) {
        return refuse(403, "Origin not allowed");
    }
    let now = (shared.clock)();
    let ip = client_ip(shared, headers, peer);
    {
        let mut limits = shared.limits.lock().expect("limits");
        if !limits.connection.allow(&ip, now) || !limits.entry.allow("rooms", now) {
            return refuse(429, "Too many room connections; try again shortly");
        }
    }
    let address = match reserve_address(shared, &ip) {
        Ok(address) => address,
        Err(refusal) => return refusal.response(),
    };
    if let Err(refusal) = room_capacity(
        shared,
        &shared.registry.lock().expect("room registry"),
        &code,
    ) {
        return refusal.response();
    }
    // The checks `ws`'s handleUpgrade makes, with its status codes and messages.
    if request.method() != Method::GET {
        return abort_handshake(StatusCode::METHOD_NOT_ALLOWED, "Invalid HTTP method", None);
    }
    if !header_text(headers, "upgrade").eq_ignore_ascii_case("websocket") {
        return abort_handshake(StatusCode::BAD_REQUEST, "Invalid Upgrade header", None);
    }
    let key = header_text(headers, "sec-websocket-key");
    if !websocket::is_valid_key(key) {
        return abort_handshake(
            StatusCode::BAD_REQUEST,
            "Missing or invalid Sec-WebSocket-Key header",
            None,
        );
    }
    if !matches!(header_text(headers, "sec-websocket-version"), "13" | "8") {
        return abort_handshake(
            StatusCode::BAD_REQUEST,
            "Missing or invalid Sec-WebSocket-Version header",
            Some(("Sec-WebSocket-Version", "13, 8")),
        );
    }
    let protocols = header_text(headers, "sec-websocket-protocol");
    let mut offered: Vec<&str> = Vec::new();
    if !protocols.is_empty() {
        for protocol in protocols.split(',').map(str::trim) {
            let token = !protocol.is_empty()
                && protocol
                    .bytes()
                    .all(|byte| byte.is_ascii_graphic() && !b"()<>@,;:\\\"/[]?={}".contains(&byte));
            if !token || offered.contains(&protocol) {
                return abort_handshake(
                    StatusCode::BAD_REQUEST,
                    "Invalid Sec-WebSocket-Protocol header",
                    None,
                );
            }
            offered.push(protocol);
        }
    }
    let extensions = headers
        .get_all(header::SEC_WEBSOCKET_EXTENSIONS)
        .iter()
        .filter_map(|value| value.to_str().ok());
    let Ok(deflate) = extension::negotiate(extensions) else {
        return abort_handshake(
            StatusCode::BAD_REQUEST,
            "Invalid or unacceptable Sec-WebSocket-Extensions header",
            None,
        );
    };
    let mut response = Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(header::UPGRADE, "websocket")
        .header(header::CONNECTION, "Upgrade")
        .header(header::SEC_WEBSOCKET_ACCEPT, websocket::accept_key(key));
    // Without a protocol handler, ws picks the first one offered.
    if let Some(protocol) = offered.first() {
        response = response.header(header::SEC_WEBSOCKET_PROTOCOL, *protocol);
    }
    if let Some(params) = &deflate {
        response = response.header(
            header::SEC_WEBSOCKET_EXTENSIONS,
            HeaderValue::from_str(&params.response_header()).expect("ASCII header"),
        );
    }
    // Node admitted the socket in the same turn as the handshake, so the next request
    // already saw the new room and the address's socket. Reserve both before answering.
    let (handle, output) = socket::socket_pair();
    let reservation = match reserve(shared, &code, &handle) {
        Ok(reservation) => reservation,
        Err(refusal) => return refusal.response(),
    };
    let upgraded = hyper::upgrade::on(request);
    let shared = shared.clone();
    tokio::spawn(async move {
        // Cancellation and every early return release this address's reserved slot.
        let _address = address;
        match upgraded.await {
            Ok(upgraded) => {
                let codec = Codec::new(Role::Server, deflate.as_ref(), MAX_FRAME_BYTES);
                let socket = PendingSocket {
                    code,
                    handle,
                    output,
                    reservation,
                };
                serve_socket(&shared, TokioIo::new(upgraded), codec, socket, bytes).await;
            }
            Err(_) => {
                // The client left before the upgrade finished: release what was reserved.
                if let Ok(Admission::Accepted(id)) = reservation.admission.await {
                    let _ = reservation
                        .room
                        .send(RoomCommand::Closed { id, code: 1006 })
                        .await;
                }
            }
        }
    });
    response.body(empty_body()).expect("valid response")
}

/// A refused upgrade's status and message.
struct Refusal(u16, &'static str);

impl Refusal {
    fn response(self) -> Response<Body> {
        refuse(self.0, self.1)
    }
}

/// Refuses a socket for a full room, or for a new room beyond `MAX_ROOMS`.
fn room_capacity(
    shared: &Shared,
    rooms: &HashMap<String, RoomHandle>,
    code: &str,
) -> Result<(), Refusal> {
    match rooms.get(code) {
        Some(room) if room.connections.load(Ordering::Relaxed) >= MAX_PENDING_CONNECTIONS => {
            Err(Refusal(429, "Room connection limit"))
        }
        None if rooms.len() >= shared.options.max_rooms
            || shared.stopping.load(Ordering::Relaxed) =>
        {
            Err(Refusal(503, "Server is full; try again later"))
        }
        _ => Ok(()),
    }
}

/// A room's pending answer to a socket's `Open`.
struct Reservation {
    room: mpsc::Sender<RoomCommand>,
    admission: oneshot::Receiver<Admission>,
}

/// Queues the socket's `Open` with its room, creating the room when needed.
fn reserve(shared: &Shared, code: &str, handle: &SocketHandle) -> Result<Reservation, Refusal> {
    let mut rooms = shared.registry.lock().expect("room registry");
    loop {
        room_capacity(shared, &rooms, code)?;
        let room = match rooms.get(code) {
            Some(room) => room.sender.clone(),
            None => {
                let instance = shared.next_instance.fetch_add(1, Ordering::Relaxed);
                let room = (shared.spawn_room)(code, instance);
                let sender = room.sender.clone();
                rooms.insert(code.to_string(), room);
                sender
            }
        };
        let (reply, admission) = oneshot::channel();
        let open = RoomCommand::Open {
            socket: handle.clone(),
            reply,
        };
        match room.try_send(open) {
            Ok(()) => return Ok(Reservation { room, admission }),
            Err(mpsc::error::TrySendError::Full(_)) => {
                return Err(Refusal(429, "Room connection limit"));
            }
            // A room leaves the registry before closing its inbox, so this entry is stale.
            Err(mpsc::error::TrySendError::Closed(_)) => {
                rooms.remove(code);
            }
        }
    }
}

/// A connection slot is reserved in the same critical section that checks its limit.
/// Dropping it also covers refused handshakes and cancelled upgrade tasks.
struct AddressReservation {
    shared: Arc<Shared>,
    ip: String,
}

fn reserve_address(shared: &Arc<Shared>, ip: &str) -> Result<AddressReservation, Refusal> {
    let mut by_ip = shared.sockets_by_ip.lock().expect("sockets by ip");
    let open = by_ip.get(ip).copied().unwrap_or(0);
    if open >= shared.options.max_sockets_per_ip {
        return Err(Refusal(429, "Too many open connections from this address"));
    }
    *by_ip.entry(ip.to_string()).or_default() += 1;
    Ok(AddressReservation {
        shared: shared.clone(),
        ip: ip.to_string(),
    })
}

impl Drop for AddressReservation {
    fn drop(&mut self) {
        let mut by_ip = self.shared.sockets_by_ip.lock().expect("sockets by ip");
        if let Some(open) = by_ip.get_mut(&self.ip) {
            *open -= 1;
            if *open == 0 {
                by_ip.remove(&self.ip);
            }
        }
    }
}

/// An upgraded socket on its way into its room.
struct PendingSocket {
    code: String,
    handle: SocketHandle,
    output: socket::SocketOutput,
    reservation: Reservation,
}

/// Runs one room socket from admission to close.
async fn serve_socket<IO>(
    shared: &Arc<Shared>,
    io: IO,
    codec: Codec,
    socket: PendingSocket,
    bytes: Arc<ConnectionBytes>,
) where
    IO: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send,
{
    let PendingSocket {
        code,
        handle,
        output,
        reservation,
    } = socket;
    // The upgraded connection's counters see compressed frames and the handshake.
    bytes.track();
    shared.open_sockets.fetch_add(1, Ordering::Relaxed);
    let admitted = match reservation.admission.await {
        Ok(Admission::Accepted(id)) => Ok(RoomLink {
            id,
            room: reservation.room,
        }),
        Ok(Admission::Full) => Err("Room connection limit"),
        // The room ended between the handshake and the socket's turn: try the code again.
        Ok(Admission::Ended) | Err(_) => admit(shared, &code, &handle).await,
    };
    let link = match admitted {
        Ok(link) => Some(link),
        Err(reason) => {
            handle.close(1013, reason);
            None
        }
    };
    let ending = socket::run(
        io,
        codec,
        output,
        link.as_ref(),
        shared.terminate.subscribe(),
    )
    .await;
    if let Some(link) = link {
        let command = match ending {
            Ending::Failed => RoomCommand::Failed { id: link.id },
            ref ending => RoomCommand::Closed {
                id: link.id,
                code: ending.code(),
            },
        };
        let _ = link.room.send(command).await;
    }
    shared.open_sockets.fetch_sub(1, Ordering::Relaxed);
}

/// Hands a socket to its room again after the one it reserved ended.
async fn admit(
    shared: &Shared,
    code: &str,
    handle: &SocketHandle,
) -> Result<RoomLink, &'static str> {
    loop {
        let Ok(Reservation { room, admission }) = reserve(shared, code, handle) else {
            return Err("Server is full");
        };
        match admission.await {
            Ok(Admission::Accepted(id)) => return Ok(RoomLink { id, room }),
            Ok(Admission::Full) => return Err("Room connection limit"),
            Ok(Admission::Ended) | Err(_) => continue,
        }
    }
}

/// Samples every room (oldest first) plus the socket and wire counters.
async fn gather(shared: &Shared) -> MonitorInput {
    let mut rooms: Vec<RoomHandle> = shared
        .registry
        .lock()
        .expect("room registry")
        .values()
        .cloned()
        .collect();
    rooms.sort_by_key(|room| room.instance);
    let mut replies = Vec::with_capacity(rooms.len());
    for room in rooms {
        let (reply, answer) = oneshot::channel();
        if room.sender.try_send(RoomCommand::Sample { reply }).is_ok() {
            replies.push(answer);
        }
    }
    let mut samples: Vec<RoomSample> = Vec::with_capacity(replies.len());
    for answer in replies {
        if let Ok(Ok(Some(sample))) = tokio::time::timeout(SAMPLE_TIMEOUT, answer).await {
            samples.push(sample);
        }
    }
    MonitorInput {
        samples,
        sockets: shared.open_sockets.load(Ordering::Relaxed) as u32,
        wire: shared.wire.total(),
    }
}

async fn sample(shared: &Shared) -> ServerStats {
    let input = gather(shared).await;
    let mut monitor = shared.monitor.lock().expect("monitor");
    let (reading, stats) = monitor.sample(input);
    shared
        .dashboard
        .lock()
        .expect("dashboard")
        .broadcast(&reading, &monitor);
    stats
}

async fn monitor_loop(shared: Arc<Shared>) {
    let mut interval = tokio::time::interval(Duration::from_millis(READING_INTERVAL_MS));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval.tick().await;
    loop {
        interval.tick().await;
        let input = gather(&shared).await;
        let mut monitor = shared.monitor.lock().expect("monitor");
        let reading = monitor.tick(input);
        shared
            .dashboard
            .lock()
            .expect("dashboard")
            .broadcast(&reading, &monitor);
    }
}

/// Measures how late the runtime runs a 10 ms timer: the interval between wake-ups, like
/// Node's event-loop delay histogram. Tokio's timer wheel has millisecond resolution, so
/// an idle server reads a fraction of a millisecond of lag.
async fn lag_probe(recorder: Arc<LagRecorder>) {
    let mut last = Instant::now();
    loop {
        tokio::time::sleep(LAG_PROBE_INTERVAL).await;
        let now = Instant::now();
        recorder.record((now - last).as_secs_f64() * 1000.0);
        last = now;
    }
}
