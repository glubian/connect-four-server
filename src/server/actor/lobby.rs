use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use actix::prelude::*;
use actix_web::Either;
use log::{debug, error};
use rand::{rngs::ThreadRng, thread_rng, Rng};
use uuid::Uuid;

use crate::{
    game::Player,
    server::{
        actor::{
            self,
            lobby_router::RemoveLobby,
            player::{
                AttachController, Disconnect, Disconnected, IncomingPickPlayer, OutgoingMessage,
            },
        },
        AppConfig,
    },
};

const PLAYER_LIST_SYNC_DEBOUNCE: Duration = Duration::from_secs(1);

#[derive(Message)]
#[rtype(result = "()")]
pub struct ConnectPlayer(pub Addr<actor::Player>);

#[derive(Message)]
#[rtype(result = "()")]
pub struct Shutdown;

pub struct Lobby {
    router: Addr<actor::LobbyRouter>,
    id: Uuid,

    host: Addr<actor::Player>,
    players: HashMap<u8, Addr<actor::Player>>,
    player_list_sync: PlayerListSync,
    rng: ThreadRng,
    game: Option<Addr<actor::Game>>,

    cfg: Arc<AppConfig>,
}

struct PlayerListSync {
    last_update: Instant,
    handle: Option<SpawnHandle>,
}

impl Lobby {
    #[must_use]
    pub fn new(
        router: Addr<actor::LobbyRouter>,
        id: Uuid,
        host: Addr<actor::Player>,
        cfg: Arc<AppConfig>,
    ) -> Self {
        Self {
            router,
            id,
            host,
            players: HashMap::new(),
            player_list_sync: PlayerListSync {
                last_update: Instant::now(),
                handle: None,
            },
            rng: thread_rng(),
            game: None,
            cfg,
        }
    }

    fn get_id(&mut self) -> Option<u8> {
        if self.players.len() == self.cfg.max_players {
            return None;
        }

        loop {
            let id = self.rng.gen_range(0..=u8::MAX);
            if !self.players.contains_key(&id) {
                return Some(id);
            }
        }
    }

    fn sync_player_list(&mut self, _: &mut actix::Context<Self>) {
        let codes: Vec<u8> = self.players.keys().copied().collect();
        let Ok(msg) = OutgoingMessage::LobbySync { players: &codes }.into_serialized() else {
            error!("Failed to serialize lobby sync message");
            return;
        };
        self.host.do_send(msg);

        let sync = &mut self.player_list_sync;
        sync.last_update = Instant::now();
        sync.handle = None;
    }

    fn schedule_player_list_sync(&mut self, ctx: &mut actix::Context<Self>) {
        let sync = &mut self.player_list_sync;
        if sync.handle.is_some() {
            return;
        }

        if sync.last_update.elapsed() < PLAYER_LIST_SYNC_DEBOUNCE {
            sync.handle = Some(ctx.run_later(PLAYER_LIST_SYNC_DEBOUNCE, Self::sync_player_list));
        } else {
            self.sync_player_list(ctx);
        }
    }
}

impl Actor for Lobby {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let msg = AttachController(Either::Left(ctx.address()));
        let Ok(()) = self.host.try_send(msg) else {
            debug!("Failed to attach controller to host, shutting down");
            ctx.stop();
            return;
        };

        let Ok(link_msg) = OutgoingMessage::lobby_link(self.id, &self.cfg).into_serialized() else {
            error!("Failed to serialize lobby link message, shutting down");
            ctx.stop();
            return;
        };

        self.host.do_send(link_msg);
        debug!("Started");
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        if self.game.is_none() {
            self.host.do_send(Disconnect::LobbyClosed);
        }

        if let Some(handle) = self.player_list_sync.handle {
            ctx.cancel_future(handle);
        }

        let disconnect_msg = if self.game.is_none() {
            Disconnect::LobbyClosed
        } else {
            Disconnect::GameStarted
        };
        for player in self.players.values() {
            player.do_send(disconnect_msg);
        }

        self.router.do_send(RemoveLobby(self.id));
        debug!("Shut down");
    }
}

impl Handler<ConnectPlayer> for Lobby {
    type Result = ();

    fn handle(&mut self, msg: ConnectPlayer, ctx: &mut Self::Context) {
        let player = msg.0;
        let Some(id) = self.get_id() else {
            player.do_send(Disconnect::LobbyFull);
            debug!("A player could not join because the lobby is full!");
            return;
        };

        let Ok(()) = player.try_send(AttachController(Either::Left(ctx.address()))) else {
            player.do_send(Disconnect::LobbyJoinError);
            debug!("Failed to attach controller to a player");
            return;
        };

        let Ok(msg) = OutgoingMessage::LobbyCode { code: id }.into_serialized() else {
            player.do_send(Disconnect::LobbyJoinError);
            error!("Failed to serialize lobby code message");
            return;
        };

        player.do_send(msg);
        self.players.insert(id, player);
        self.schedule_player_list_sync(ctx);
        debug!("Player {} has joined", id);
    }
}

impl Handler<Disconnected> for Lobby {
    type Result = ();

    fn handle(&mut self, msg: Disconnected, ctx: &mut Self::Context) {
        if self.game.is_some() {
            return;
        }

        if msg.0 == self.host.downgrade() {
            debug!("Host has disconnected; lobby shutting down");
            ctx.stop();
            return;
        }

        self.players.retain(|_, player| player.connected());
        self.schedule_player_list_sync(ctx);
        debug!("Player left");
    }
}

impl Handler<IncomingPickPlayer> for Lobby {
    type Result = ();

    fn handle(&mut self, msg: IncomingPickPlayer, ctx: &mut Self::Context) {
        let IncomingPickPlayer {
            code,
            game,
            config,
            round,
            role,
        } = msg;
        let Some(player) = self.players.remove(&code) else { return; };
        let (p1, p2) = match role {
            Player::P1 => (player, self.host.clone()),
            Player::P2 => (self.host.clone(), player),
        };
        let cfg = Arc::clone(&self.cfg);
        let game = actor::Game::new(game, config.into(), round, p1, p2, cfg);
        self.game = Some(game.start());
        debug!(
            "Player {} was chosen as {:?}, lobby shutting down",
            msg.code, msg.role
        );

        ctx.stop();
    }
}

impl Handler<Shutdown> for Lobby {
    type Result = ();

    fn handle(&mut self, _: Shutdown, ctx: &mut Self::Context) {
        debug!("Lobby shutting down");
        ctx.stop();
    }
}
