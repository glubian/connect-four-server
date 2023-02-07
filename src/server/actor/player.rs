use std::{sync::Arc, time::Instant};

use actix::{prelude::*, WeakAddr};
use actix_web::Either;
use actix_web_actors::ws::{self, CloseReason};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::game::{self, Game};
use crate::server::{
    actor::{
        self,
        game::{EndTurn, Restart},
        lobby::PickPlayer,
    },
    config::AppConfig,
};

// Outgoing messages

#[derive(Message)]
#[rtype(result = "()")]
pub struct LobbyLink(pub Uuid);

#[derive(Message)]
#[rtype(result = "()")]
pub struct LobbySync(pub Vec<u8>);

#[derive(Message)]
#[rtype(result = "()")]
pub struct LobbyCode(pub u8);

#[derive(Message, Clone)]
#[rtype(result = "()")]
pub struct GameRole(pub game::Player);

#[derive(Message, Clone)]
#[rtype(result = "()")]
pub struct GameSync {
    round: u32,
    game: Arc<String>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum OutgoingMessage<'a> {
    LobbyLink(OutgoingLobbyLink<'a>),
    LobbySync { players: &'a [u8] },
    LobbyCode { code: u8 },
    GameRole { role: game::Player },
    GameSync { round: u32, game: &'a str },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OutgoingLobbyLink<'a> {
    lobby: &'a str,
    qr_code: QR,
}

impl<'a> From<OutgoingLobbyLink<'a>> for OutgoingMessage<'a> {
    fn from(value: OutgoingLobbyLink<'a>) -> Self {
        Self::LobbyLink(value)
    }
}

#[derive(Serialize, Default)]
struct QR {
    img: String,
    width: usize,
}

// Incoming messages

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum IncomingMessage {
    LobbyPickPlayer(PickPlayer),
    GameEndTurn { col: usize },
    GameRestart,
}

// Internal messages

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

impl Handler<AttachController> for Player {
    type Result = ();

    fn handle(&mut self, msg: AttachController, _: &mut Self::Context) {
        self.controller = Some(msg.0);
        debug!("Controller attached");
    }
}

// LobbyLink utilities

fn generate_lobby_url(app_config: &AppConfig, lobby_id: &str) -> String {
    use qstring::QString;
    let mut url = app_config.url_base.clone();
    let query = QString::new(vec![(&app_config.url_lobby_parameter, lobby_id)]);
    url.set_query(Some(&query.to_string()));
    url.into()
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

impl Handler<LobbyLink> for Player {
    type Result = ();

    fn handle(&mut self, msg: LobbyLink, ctx: &mut Self::Context) {
        let mut buf = Uuid::encode_buffer();
        let lobby_id = msg.0.hyphenated().encode_lower(&mut buf);

        let qr_code = QR::generate(&generate_lobby_url(&self.cfg, lobby_id)).unwrap_or_default();
        let msg: OutgoingMessage = OutgoingLobbyLink {
            lobby: lobby_id,
            qr_code,
        }
        .into();

        let Ok(msg) = serde_json::to_string(&msg) else {
            error!("Failed to convert lobby link message");
            return;
        };

        ctx.text(msg);
        debug!("Lobby link sent");
    }
}

impl Handler<LobbySync> for Player {
    type Result = ();

    fn handle(&mut self, msg: LobbySync, ctx: &mut Self::Context) {
        let Ok(msg) = serde_json::to_string(&OutgoingMessage::LobbySync { players: &msg.0 }) else {
            error!("Failed to convert lobby sync message");
            return;
        };

        ctx.text(msg);
        debug!("Lobby sync sent");
    }
}

impl Handler<LobbyCode> for Player {
    type Result = ();

    fn handle(&mut self, msg: LobbyCode, ctx: &mut Self::Context) {
        let Ok(msg) = serde_json::to_string(&OutgoingMessage::LobbyCode { code: msg.0 }) else {
            error!("Failed to convert lobby code message");
            return;
        };

        ctx.text(msg);
        debug!("Lobby code sent");
    }
}

impl Handler<GameRole> for Player {
    type Result = ();

    fn handle(&mut self, msg: GameRole, ctx: &mut Self::Context) {
        let Ok(msg) = serde_json::to_string(&OutgoingMessage::GameRole { role: msg.0 }) else {
            error!("Failed to convert game role message");
            return;
        };

        ctx.text(msg);
        debug!("Game role sent");
    }
}

impl Handler<GameSync> for Player {
    type Result = ();

    fn handle(&mut self, msg: GameSync, ctx: &mut Self::Context) {
        let round = msg.round;
        let game = &msg.game;
        let Ok(msg) = serde_json::to_string(&OutgoingMessage::GameSync { round, game }) else {
            error!("Failed to convert game sync message to JSON");
            return;
        };

        ctx.text(msg);
        debug!("Game sync sent");
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
                Ok(IncomingMessage::GameEndTurn { col }) => {
                    let Some(Right(game)) = &self.controller else {
                        debug!("Received IncomingMessage::LobbyPickPlayer, but no controller is attached!");
                        return;
                    };
                    debug!("Received IncomingMessage::GameEndTurn");
                    game.try_send(EndTurn {
                        col,
                        player: ctx.address(),
                    })
                    .unwrap();
                }
                Ok(IncomingMessage::GameRestart) => {
                    let Some(Right(game)) = &self.controller else {
                        debug!("Received IncomingMessage::LobbyPickPlayer, but no controller is attached!");
                        return;
                    };
                    debug!("Received IncomingMessage::GameRestart");
                    game.try_send(Restart).unwrap();
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

impl GameSync {
    pub fn new(round: u32, game: &Game) -> Result<Self, serde_json::Error> {
        Ok(Self {
            round,
            game: Arc::new(serde_json::to_string(game)?),
        })
    }
}
