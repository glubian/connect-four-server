use std::time::Duration;
use std::{sync::Arc, time::Instant};

use actix::{prelude::*, WeakAddr};
use actix_web::Either;
use actix_web_actors::ws::{self, CloseReason};
use bytestring::ByteString;
use chrono::{DateTime, Utc};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::game::{self, Game};
use crate::game_config::{GameConfig, PartialGameConfig};
use crate::server::actor::game::{PlayerSelectionVote, RestartResponse};
use crate::server::serde::as_millis_optional_tuple;
use crate::server::{
    actor::{
        self,
        game::{EndTurn, Restart},
    },
    config::AppConfig,
};

const ISO_8601_TIMESTAMP: &str = "%Y-%m-%dT%H:%M:%S%.3fZ";

// Outgoing messages

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum OutgoingMessage<'a> {
    LobbyLink(OutgoingLobbyLink),
    LobbySync { players: &'a [u8] },
    LobbyCode { code: u8 },
    GameSetup(OutgoingGameSetup<'a>),
    GamePlayerSelection(OutgoingPlayerSelection),
    GameSync(OutgoingGameSync<'a>),
    GameRestartRequest(OutgoingRestartRequest<'a>),
    Pong { sent: f64, received: String },
}

impl<'a> OutgoingMessage<'a> {
    /// Constructs a new `OutgoingMessage::LobbyLink`.
    #[must_use]
    pub fn lobby_link(uuid: Uuid, cfg: &AppConfig) -> Self {
        OutgoingLobbyLink::new(uuid, cfg).into()
    }

    /// Constructs a new `OutgoingMessage::GameSetup`, containing all fields.
    #[must_use]
    pub fn full_game_setup(role: game::Player, config: &'a GameConfig) -> Self {
        OutgoingGameSetup::new()
            .set_role(role)
            .set_config(config)
            .set_timestamp()
            .into()
    }

    /// Returns an `OutgoingMessage::GameSetup` builder.
    #[must_use]
    pub const fn game_setup() -> OutgoingGameSetup<'a> {
        OutgoingGameSetup::new()
    }

    /// Constructs a new `OutgoingMessage::GamePlayerSelection`.
    #[must_use]
    pub fn game_player_selection(p1_voted: bool, p2_voted: bool) -> Self {
        OutgoingPlayerSelection { p1_voted, p2_voted }.into()
    }

    /// Constructs a new `OutgoingMessage::GameSync`.
    #[must_use]
    pub fn game_sync(round: u32, game: &'a Game, timeout: Option<DateTime<Utc>>) -> Self {
        OutgoingGameSync::new(round, game, timeout).into()
    }

    /// Constructs a new `OutgoingMessage::GameRestartRequest`.
    #[must_use]
    pub fn game_restart_request(player: game::Player, req: Option<RestartRequest<'a>>) -> Self {
        OutgoingRestartRequest { player, req }.into()
    }

    /// Attempts to convert the message into a `SerializedOutgoingMessage`.
    pub fn into_serialized(self) -> Result<SerializedOutgoingMessage, serde_json::Error> {
        self.try_into()
    }

    /// Attempts to convert the message into a `SharedOutgoingMessage`.
    pub fn into_shared(self) -> Result<SharedOutgoingMessage, serde_json::Error> {
        self.try_into()
    }
}

/// Contents of `OutgoingMessage::LobbyLink`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingLobbyLink {
    lobby: String,
    qr_code: QR,
}

impl OutgoingLobbyLink {
    #[must_use]
    pub fn new(uuid: Uuid, cfg: &AppConfig) -> Self {
        fn generate_lobby_url(app_config: &AppConfig, lobby_id: &str) -> String {
            use qstring::QString;
            let mut url = app_config.url_base.clone();
            let query = QString::new(vec![(&app_config.url_lobby_parameter, lobby_id)]);
            url.set_query(Some(&query.to_string()));
            url.into()
        }

        let lobby = uuid.as_hyphenated().to_string();
        let qr_code = QR::generate(&generate_lobby_url(cfg, &lobby)).unwrap_or_default();
        Self { lobby, qr_code }
    }
}

impl<'a> From<OutgoingLobbyLink> for OutgoingMessage<'a> {
    fn from(msg: OutgoingLobbyLink) -> Self {
        Self::LobbyLink(msg)
    }
}

/// Contents of `OutgoingMessage::GameSetup` with builder functions for
/// setting fields.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingGameSetup<'a> {
    /// Tells the client which player controls it - `P1` (blue) or `P2` (red)
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<game::Player>,
    /// Changes the configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<&'a GameConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<String>,
}

impl<'a> OutgoingGameSetup<'a> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            role: None,
            config: None,
            timestamp: None,
        }
    }

    #[must_use]
    pub const fn set_role(mut self, role: game::Player) -> Self {
        self.role = Some(role);
        self
    }

    #[must_use]
    pub const fn set_config(mut self, config: &'a GameConfig) -> Self {
        self.config = Some(config);
        self
    }

    #[must_use]
    pub fn set_timestamp(mut self) -> Self {
        self.timestamp = Some(Utc::now().format(ISO_8601_TIMESTAMP).to_string());
        self
    }
}

impl<'a> From<OutgoingGameSetup<'a>> for OutgoingMessage<'a> {
    fn from(msg: OutgoingGameSetup<'a>) -> Self {
        Self::GameSetup(msg)
    }
}

/// Contents of `OutgoingMessage::PlayerSelection`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingPlayerSelection {
    pub p1_voted: bool,
    pub p2_voted: bool,
}

impl<'a> From<OutgoingPlayerSelection> for OutgoingMessage<'a> {
    fn from(msg: OutgoingPlayerSelection) -> Self {
        Self::GamePlayerSelection(msg)
    }
}

/// Contents of `OutgoingMessage::GameSync`.k
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingGameSync<'a> {
    round: u32,
    game: &'a Game,
    /// ISO 8601 timestamp of when the turn will be ended automatically.
    timeout: Option<String>,
}

impl<'a> OutgoingGameSync<'a> {
    #[must_use]
    pub fn new(round: u32, game: &'a Game, timeout: Option<DateTime<Utc>>) -> Self {
        Self {
            round,
            game,
            timeout: timeout.map(|t| t.format(ISO_8601_TIMESTAMP).to_string()),
        }
    }
}

impl<'a> From<OutgoingGameSync<'a>> for OutgoingMessage<'a> {
    fn from(msg: OutgoingGameSync<'a>) -> Self {
        Self::GameSync(msg)
    }
}

/// Updates the status of restart request of the given player.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingRestartRequest<'a> {
    /// Player who made the request.
    player: game::Player,
    /// Restart request details; `None` if it expired.
    #[serde(skip_serializing_if = "Option::is_none")]
    req: Option<RestartRequest<'a>>,
}

impl<'a> From<OutgoingRestartRequest<'a>> for OutgoingMessage<'a> {
    fn from(msg: OutgoingRestartRequest<'a>) -> Self {
        Self::GameRestartRequest(msg)
    }
}

/// Restart request made when the game cannot be restarted without asking
/// the permission of the opponent first.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestartRequest<'a> {
    /// Changed configuration, if any.
    config: Option<&'a GameConfig>,
    /// ISO 8601 timestamp of when the restart request will expire.
    timeout: String,
}

impl<'a> RestartRequest<'a> {
    #[must_use]
    pub fn new(config: Option<&'a GameConfig>, timeout: DateTime<Utc>) -> Self {
        let timeout = timeout.format(ISO_8601_TIMESTAMP).to_string();
        Self { config, timeout }
    }
}

/// QR code representation sent over to the client.
#[derive(Serialize, Default)]
struct QR {
    /// Base64-encoded PNG.
    img: String,
    /// The number of modules per side.
    width: usize,
}

impl QR {
    /// Attempts to generate a QR code with specified contents.
    fn generate(contents: &str) -> Result<Self, ()> {
        use image::{png::PngEncoder, ColorType, Luma};
        use qrcode::{EcLevel, QrCode};
        let mut img = Vec::new();

        let qr = QrCode::with_error_correction_level(contents, EcLevel::L).map_err(|_| ())?;
        let img_buf = qr
            .render::<Luma<u8>>()
            .max_dimensions(200, 200)
            .quiet_zone(false)
            .build();

        PngEncoder::new(&mut img)
            .encode(&img_buf, img_buf.width(), img_buf.height(), ColorType::L8)
            .map_err(|_| ())?;

        Ok(Self {
            img: base64::encode(&img),
            width: qr.width(),
        })
    }
}

// Incoming messages

/// Contents of `IncomingMessage::LobbyPickPlayer`.
#[derive(Message, Deserialize)]
#[serde(rename_all = "camelCase")]
#[rtype(result = "()")]
pub struct IncomingPickPlayer {
    /// Player code.
    pub code: u8,
    /// Role which should be assigned to the player.
    pub role: game::Player,
    /// State of the local game, or `None` if the client is in player selection.
    pub game: Option<Game>,
    /// Game configuration, any missing fields will be set to their default value.
    pub config: PartialGameConfig,
    pub round: u32,
    /// In timed games, the extra time each player has in milliseconds.
    #[serde(with = "as_millis_optional_tuple", default)]
    pub extra_time: Option<[Duration; 2]>,
}

/// Contents of `IncomingMessage::GamePlayerSelectionVote`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncomingPlayerSelectionVote {
    wants_to_start: bool,
}

/// Contents of `IncomingMessage::GameEndTurn`.
#[derive(Deserialize)]
struct IncomingEndTurn {
    /// The turn the player wants to end.
    turn: u32,
    /// Move the player wants to make, if any.
    #[serde(default)]
    col: Option<usize>,
}

/// Contents of `IncomingMessage::GameRestart`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncomingRestart {
    /// Changes to the configuration, if any.
    #[serde(flatten)]
    partial: Option<PartialGameConfig>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum IncomingMessage {
    LobbyPickPlayer(IncomingPickPlayer),
    GamePlayerSelectionVote(IncomingPlayerSelectionVote),
    GameEndTurn(IncomingEndTurn),
    GameRestart(IncomingRestart),
    GameRestartResponse { accepted: bool },
    Ping { sent: f64 },
}

// Internal messages

#[derive(Message)]
#[rtype(result = "()")]
pub struct SerializedOutgoingMessage(String);

impl<'a> TryFrom<OutgoingMessage<'a>> for SerializedOutgoingMessage {
    type Error = serde_json::Error;

    fn try_from(msg: OutgoingMessage) -> Result<Self, Self::Error> {
        Ok(Self(serde_json::to_string(&msg)?))
    }
}

/// Stores the converted message as an `Arc<String>`, allowing it to be sent to
/// multiple players.
#[derive(Message, Clone)]
#[rtype(result = "()")]
pub struct SharedOutgoingMessage(Arc<String>);

impl<'a> TryFrom<OutgoingMessage<'a>> for SharedOutgoingMessage {
    type Error = serde_json::Error;

    fn try_from(msg: OutgoingMessage) -> Result<Self, Self::Error> {
        let msg = serde_json::to_string(&msg)?;
        Ok(Self(Arc::new(msg)))
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct AttachController(pub Either<Addr<actor::Lobby>, Addr<actor::Game>>);

#[derive(Message)]
#[rtype(result = "()")]
pub struct Disconnected(pub WeakAddr<Player>);

#[derive(Serialize, Message, Clone, Copy)]
#[serde(rename_all = "camelCase")]
#[rtype(result = "()")]
pub enum Disconnect {
    ServerMaxLobbies,
    InviteInvalid,
    LobbyJoinError,
    LobbyFull,
    LobbyClosed,
    GameStarted,
    GameEnded,
    LobbyOverloaded,
    ServerOverloaded,
    ShuttingDown,
}

impl Disconnect {
    fn as_str(&self) -> &str {
        match self {
            Self::ServerMaxLobbies => "serverMaxLobbies",
            Self::InviteInvalid => "inviteInvalid",
            Self::LobbyJoinError => "lobbyJoinError",
            Self::LobbyFull => "lobbyFull",
            Self::LobbyClosed => "lobbyClosed",
            Self::GameStarted => "gameStarted",
            Self::GameEnded => "gameEnded",
            Self::LobbyOverloaded => "lobbyOverloaded",
            Self::ServerOverloaded => "serverOverloaded",
            Self::ShuttingDown => "shuttingDown",
        }
    }
}

pub struct Player {
    hb: Instant,
    controller: Option<Either<Addr<actor::Lobby>, Addr<actor::Game>>>,
    disconnected_by_controller: bool,
    cfg: Arc<AppConfig>,
}

impl Player {
    fn hb(&self, ctx: &mut ws::WebsocketContext<Self>) {
        if self.cfg.heartbeat_interval.is_zero() || self.cfg.heartbeat_timeout.is_zero() {
            return;
        }

        let timeout = self.cfg.heartbeat_timeout;
        ctx.run_interval(self.cfg.heartbeat_interval, move |actor, ctx| {
            if Instant::now().duration_since(actor.hb) > timeout {
                ctx.stop();
                debug!("Timed out");
            }
        });
    }

    #[must_use]
    pub fn new(app_config: Arc<AppConfig>) -> Self {
        Self {
            hb: Instant::now(),
            controller: None,
            disconnected_by_controller: false,
            cfg: app_config,
        }
    }

    fn handle_text_message(&mut self, text: &ByteString, ctx: &mut ws::WebsocketContext<Self>) {
        use Either::*;

        let Ok(msg) = serde_json::from_str::<IncomingMessage>(text) else {
            debug!("Failed to parse message!");
            return;
        };

        self.hb = Instant::now();

        match msg {
            IncomingMessage::LobbyPickPlayer(msg) => {
                let Some(Left(lobby)) = &self.controller else {
                    debug!("Received IncomingMessage::LobbyPickPlayer, but no controller is attached!");
                    return;
                };
                debug!("Received IncomingMessage::LobbyPickPlayer");
                lobby.do_send(msg);
            }
            IncomingMessage::GamePlayerSelectionVote(msg) => {
                let Some(Right(game)) = &self.controller else {
                    debug!("Received IncomingMessage::GamePlayerSelectionVote, but no controller is attached!");
                    return;
                };
                debug!("Received IncomingMessage::GamePlayerSelectionVote");
                game.do_send(PlayerSelectionVote {
                    player: ctx.address(),
                    wants_to_start: msg.wants_to_start,
                });
            }
            IncomingMessage::GameEndTurn(IncomingEndTurn { turn, col }) => {
                let Some(Right(game)) = &self.controller else {
                    debug!("Received IncomingMessage::GameEndTurn, but no controller is attached!");
                    return;
                };
                debug!("Received IncomingMessage::GameEndTurn");
                game.do_send(EndTurn {
                    player: ctx.address(),
                    turn,
                    col,
                });
            }
            IncomingMessage::GameRestart(IncomingRestart { partial }) => {
                let Some(Right(game)) = &self.controller else {
                    debug!("Received IncomingMessage::GameRestart, but no controller is attached!");
                    return;
                };
                debug!("Received IncomingMessage::GameRestart");
                game.do_send(Restart {
                    addr: ctx.address(),
                    partial,
                });
            }
            IncomingMessage::GameRestartResponse { accepted } => {
                let Some(Right(game)) = &self.controller else {
                    debug!("Received IncomingMessage::GameRestartVote, but no controller is attached!");
                    return;
                };
                debug!("Received IncomingMessage::GameRestartVote");
                game.do_send(RestartResponse {
                    addr: ctx.address(),
                    accepted,
                });
            }
            IncomingMessage::Ping { sent } => {
                let received = Utc::now().format(ISO_8601_TIMESTAMP).to_string();
                // Fail silently just to be safe
                let Ok(msg) = serde_json::to_string(&OutgoingMessage::Pong { sent, received }) else {
                    debug!("Failed to serialize message");
                    return;
                };
                ctx.text(msg);
            }
        }
    }
}

impl Actor for Player {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.hb(ctx);
        debug!("Started");
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        use Either::*;

        if self.disconnected_by_controller {
            debug!("Shut down by controller");
            return;
        }

        let weak_addr = ctx.address().downgrade();

        match &self.controller {
            Some(Left(lobby)) => lobby.do_send(Disconnected(weak_addr)),
            Some(Right(game)) => game.do_send(Disconnected(weak_addr)),
            None => {
                debug!("Shut down, no controller was attached");
                return;
            }
        };

        debug!("Shut down, controller has been notified");
    }
}

// Handlers

impl Handler<AttachController> for Player {
    type Result = ();

    fn handle(&mut self, msg: AttachController, _: &mut Self::Context) {
        self.controller = Some(msg.0);
        debug!("Controller attached");
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for Player {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        let Ok(msg) = msg else {
            error!("WebSocket protocol error");
            ctx.stop();
            return;
        };

        match msg {
            ws::Message::Text(text) => self.handle_text_message(&text, ctx),
            ws::Message::Continuation(_) => {
                ctx.close(Some(ws::CloseCode::Unsupported.into()));
                ctx.stop();
            }
            ws::Message::Close(reason) => {
                debug!("Connection closed");
                ctx.close(reason);
                ctx.stop();
            }
            ws::Message::Ping(_)
            | ws::Message::Pong(_)
            | ws::Message::Binary(_)
            | ws::Message::Nop => (),
        }
    }
}

impl Handler<Disconnect> for Player {
    type Result = ();

    fn handle(&mut self, d: Disconnect, ctx: &mut Self::Context) {
        debug!("Controller disconnected");
        self.disconnected_by_controller = true;
        ctx.close(Some(CloseReason {
            code: ws::CloseCode::Normal,
            description: Some(String::from(d.as_str())),
        }));
        ctx.stop();
    }
}

impl Handler<SerializedOutgoingMessage> for Player {
    type Result = ();

    fn handle(&mut self, msg: SerializedOutgoingMessage, ctx: &mut Self::Context) {
        ctx.text(&msg.0[..]);
    }
}

impl Handler<SharedOutgoingMessage> for Player {
    type Result = ();

    fn handle(&mut self, msg: SharedOutgoingMessage, ctx: &mut Self::Context) {
        ctx.text(msg.0.as_str());
        debug!("Shared outgoing message sent");
    }
}
