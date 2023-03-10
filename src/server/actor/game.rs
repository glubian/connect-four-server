use actix::prelude::*;
use actix_web::Either;
use log::debug;

use crate::game::Game as InternalGame;
use crate::game::{GameRules, Player};

use crate::server::actor;
use actor::player::{
    AttachController, Disconnect, Disconnected, GameRole, OutgoingMessage, SharedOutgoingMessage,
};

#[derive(Message)]
#[rtype(result = "()")]
pub struct EndTurn {
    pub player: Addr<actor::Player>,
    pub turn: u32,
    pub col: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Restart;

pub struct Game {
    game: InternalGame,
    round: u32,
    p1: Addr<actor::Player>,
    p2: Addr<actor::Player>,
}

impl Game {
    #[must_use]
    pub fn new(
        game: InternalGame,
        round: u32,
        p1: Addr<actor::Player>,
        p2: Addr<actor::Player>,
    ) -> Self {
        Self {
            game,
            round,
            p1,
            p2,
        }
    }

    fn current_player_addr(&self) -> &Addr<actor::Player> {
        match self.game.state().player {
            Player::P1 => &self.p1,
            Player::P2 => &self.p2,
        }
    }

    fn sync(&self) {
        let round = self.round;
        let game = &self.game;
        let sync1: SharedOutgoingMessage = OutgoingMessage::GameSync { round, game }
            .try_into()
            .unwrap();
        let sync2 = sync1.clone();
        self.p1.do_send(sync1);
        self.p2.do_send(sync2);
    }
}

impl Actor for Game {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let res1 = self
            .p1
            .try_send(AttachController(Either::Right(ctx.address())));
        let res2 = self
            .p2
            .try_send(AttachController(Either::Right(ctx.address())));
        if res1.is_err() || res2.is_err() {
            // both controller must be registered successfully in order for WsGame to work properly
            debug!("Failed to attach controller, shutting down");
            ctx.stop();
            return;
        }

        self.p1.do_send(GameRole(Player::P1));
        self.p2.do_send(GameRole(Player::P2));
        self.sync();
        debug!("Started");
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        debug!("Shutting down");
        self.p1.do_send(Disconnect::GameEnded);
        self.p2.do_send(Disconnect::GameEnded);
    }
}

impl Handler<Disconnected> for Game {
    type Result = ();

    fn handle(&mut self, _: Disconnected, ctx: &mut Self::Context) {
        ctx.stop();
    }
}

impl Handler<EndTurn> for Game {
    type Result = ();

    fn handle(&mut self, msg: EndTurn, _: &mut Self::Context) {
        let turn = self.game.state().turn;
        let current_player_addr = self.current_player_addr();

        if &msg.player == current_player_addr
            && turn == msg.turn
            && self.game.end_turn(msg.col).is_ok()
        {
            self.sync();
        }
    }
}

impl Handler<Restart> for Game {
    type Result = ();

    fn handle(&mut self, _: Restart, _: &mut Self::Context) {
        let rules = self.game.rules();
        let rules = GameRules {
            starting_player: rules.starting_player.other(),
            allow_draws: rules.allow_draws,
        };
        self.game = InternalGame::new(rules);
        self.round = self.round.wrapping_add(1);
        self.sync();
        debug!("Restarted");
    }
}
