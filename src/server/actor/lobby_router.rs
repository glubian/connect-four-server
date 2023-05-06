use std::{collections::HashMap, sync::Arc};

use actix::prelude::*;
use log::debug;
use uuid::Uuid;

use crate::server::{actor, AppConfig};
use actor::lobby::{ConnectPlayer, Shutdown};
use actor::player::Disconnect;

#[derive(Message)]
#[rtype(result = "()")]
pub struct CreateLobby {
    pub host: Addr<actor::Player>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct JoinLobby {
    pub id: Uuid,
    pub player: Addr<actor::Player>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RemoveLobby(pub Uuid);

pub struct LobbyRouter {
    lobbies: HashMap<Uuid, Addr<actor::Lobby>>,
    cfg: Arc<AppConfig>,
}

impl LobbyRouter {
    #[must_use]
    pub fn new(cfg: Arc<AppConfig>) -> Self {
        Self {
            lobbies: HashMap::new(),
            cfg,
        }
    }
}

impl Actor for LobbyRouter {
    type Context = actix::Context<Self>;

    fn stopped(&mut self, _: &mut Self::Context) {
        debug!("Shutting down all lobbies");
        self.lobbies.values().for_each(|v| v.do_send(Shutdown));
    }
}

impl Handler<CreateLobby> for LobbyRouter {
    type Result = ();

    fn handle(&mut self, msg: CreateLobby, ctx: &mut Self::Context) {
        if self.lobbies.len() >= self.cfg.max_lobbies {
            debug!("Failed to create a new lobby: max capacity reached!");
            msg.host.do_send(Disconnect::ServerMaxLobbies);
            return;
        }

        let id = Uuid::new_v4();
        let addr = actor::Lobby::new(ctx.address(), id, msg.host, Arc::clone(&self.cfg)).start();
        self.lobbies.insert(id, addr);
        debug!("Created a new lobby {}", id);
    }
}

impl Handler<JoinLobby> for LobbyRouter {
    type Result = ();

    fn handle(&mut self, msg: JoinLobby, _: &mut Self::Context) {
        let Some(lobby) = self.lobbies.get(&msg.id) else {
            msg.player.do_send(Disconnect::InviteInvalid);
            debug!("Lobby {} does not exist!", msg.id);
            return;
        };

        match lobby.try_send(ConnectPlayer(msg.player.clone())) {
            Ok(()) => (),
            Err(SendError::Full(_)) => msg.player.do_send(Disconnect::LobbyOverloaded),
            Err(SendError::Closed(_)) => msg.player.do_send(Disconnect::InviteInvalid),
        }
    }
}

impl Handler<RemoveLobby> for LobbyRouter {
    type Result = ();

    fn handle(&mut self, msg: RemoveLobby, _: &mut Self::Context) {
        if let Some(lobby) = self.lobbies.remove(&msg.0) {
            if lobby.connected() {
                lobby.do_send(Shutdown);
            }

            debug!("Lobby {} removed", msg.0);
        }
    }
}
