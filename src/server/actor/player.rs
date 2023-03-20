use std::{sync::Arc, time::Instant};

use actix::{prelude::*, WeakAddr};
use actix_web::Either;
use actix_web_actors::ws::{self, CloseReason};
use chrono::Utc;
use log::{debug, error};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::game::{self, Game};
use crate::game_config::{GameConfig, PartialGameConfig};
use crate::server::actor::game::PlayerSelectionVote;
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
    GameSync { round: u32, game: &'a Game },
}

impl<'a> OutgoingMessage<'a> {
    /// Constructs a new `OutgoingMessage::LobbyLink`.
    #[must_use]
    pub fn lobby_link(uuid: Uuid, cfg: &AppConfig) -> Self {
        OutgoingLobbyLink::new(uuid, cfg).into()
    }

    #[must_use]
    pub fn full_game_setup(role: game::Player, config: &'a GameConfig) -> Self {
        OutgoingGameSetup::new()
            .set_role(role)
            .set_config(config)
            .set_timestamp()
            .into()
    }

    #[must_use]
    pub const fn game_setup() -> OutgoingGameSetup<'a> {
        OutgoingGameSetup::new()
    }

    /// Constructs a new `OutgoingMessage::GamePlayerSelection`.
    #[must_use]
    pub fn game_player_selection(p1_voted: bool, p2_voted: bool) -> Self {
        OutgoingPlayerSelection { p1_voted, p2_voted }.into()
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingGameSetup<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<game::Player>,
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

#[derive(Serialize, Default)]
struct QR {
    img: String,
    width: usize,
}

impl QR {
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

#[derive(Message, Deserialize)]
#[rtype(result = "()")]
pub struct IncomingPickPlayer {
    pub code: u8,
    pub role: game::Player,
    pub game: Option<Game>,
    pub config: PartialGameConfig,
    pub round: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncomingPlayerSelectionVote {
    wants_to_start: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncomingRestart {
    #[serde(flatten)]
    config: Option<PartialGameConfig>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum IncomingMessage {
    LobbyPickPlayer(IncomingPickPlayer),
    GamePlayerSelectionVote(IncomingPlayerSelectionVote),
    GameEndTurn { turn: u32, col: usize },
    GameRestart(IncomingRestart),
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
                return;
            }

            ctx.ping(b"");
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
        use Either::*;

        let Ok(msg) = msg else {
            error!("WebSocket protocol error");
            ctx.stop();
            return;
        };

        match msg {
            ws::Message::Ping(msg) => ctx.pong(&msg),
            ws::Message::Pong(_) => self.hb = Instant::now(),
            ws::Message::Text(text) => match serde_json::from_str::<IncomingMessage>(&text) {
                Ok(IncomingMessage::LobbyPickPlayer(msg)) => {
                    let Some(Left(lobby)) = &self.controller else {
                        debug!("Received IncomingMessage::LobbyPickPlayer, but no controller is attached!");
                        return;
                    };
                    debug!("Received IncomingMessage::LobbyPickPlayer");
                    lobby.try_send(msg).unwrap();
                }
                Ok(IncomingMessage::GamePlayerSelectionVote(msg)) => {
                    let Some(Right(game)) = &self.controller else {
                        debug!("Received IncomingMessage::GamePlayerSelectionVote, but no controller is attached!");
                        return;
                    };
                    debug!("Received IncomingMessage::GamePlayerSelectionVote");
                    game.try_send(PlayerSelectionVote {
                        player: ctx.address(),
                        wants_to_start: msg.wants_to_start,
                    })
                    .unwrap();
                }
                Ok(IncomingMessage::GameEndTurn { turn, col }) => {
                    let Some(Right(game)) = &self.controller else {
                        debug!("Received IncomingMessage::GameEndTurn, but no controller is attached!");
                        return;
                    };
                    debug!("Received IncomingMessage::GameEndTurn");
                    game.try_send(EndTurn {
                        player: ctx.address(),
                        turn,
                        col,
                    })
                    .unwrap();
                }
                Ok(IncomingMessage::GameRestart(IncomingRestart { config })) => {
                    let Some(Right(game)) = &self.controller else {
                        debug!("Received IncomingMessage::GameRestart, but no controller is attached!");
                        return;
                    };
                    debug!("Received IncomingMessage::GameRestart");
                    game.try_send(Restart(config)).unwrap();
                }
                Err(_) => debug!("Failed to parse message"),
            },
            ws::Message::Binary(_) => (),
            ws::Message::Close(reason) => {
                debug!("Connection closed");
                ctx.close(reason);
                ctx.stop();
            }
            ws::Message::Continuation(_) => {
                ctx.close(Some(ws::CloseCode::Unsupported.into()));
                ctx.stop();
            }
            ws::Message::Nop => (),
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
