use actix::prelude::*;
use actix_web::Either;
use log::{debug, error};
use rand::Rng;

use crate::game::Game as InternalGame;
use crate::game::{GameRules, Player};

use crate::game_config::GameConfig;
use crate::server::actor;
use actor::player::{
    AttachController, Disconnect, Disconnected, OutgoingMessage, OutgoingPlayerSelection,
    SharedOutgoingMessage,
};

#[derive(Message)]
#[rtype(result = "()")]
pub struct PlayerSelectionVote {
    pub player: Addr<actor::Player>,
    pub wants_to_start: bool,
}

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

struct PlayerSelectionStage {
    p1_vote: Option<bool>,
    p2_vote: Option<bool>,
    config: GameConfig,
}

impl PlayerSelectionStage {
    fn new(config: GameConfig) -> Self {
        Self {
            p1_vote: None,
            p2_vote: None,
            config,
        }
    }
}

struct InGameStage {
    game: InternalGame,
}

impl InGameStage {
    fn starting_player(p1_vote: bool, p2_vote: bool) -> Player {
        if p1_vote && !p2_vote {
            Player::P1
        } else if p2_vote && !p1_vote {
            Player::P2
        } else if rand::thread_rng().gen::<bool>() {
            Player::P1
        } else {
            Player::P2
        }
    }

    fn new(p1_vote: bool, p2_vote: bool, rules: &GameConfig) -> Self {
        let starting_player = Self::starting_player(p1_vote, p2_vote);
        let rules = GameRules {
            starting_player,
            allow_draws: rules.allow_draws,
        };
        let game = InternalGame::new(rules);
        Self { game }
    }
}

enum GameStage {
    PlayerSelection(PlayerSelectionStage),
    InGame(InGameStage),
}

impl GameStage {
    fn outgoing_message(&self, round: u32) -> OutgoingMessage {
        match self {
            Self::PlayerSelection(stage) => {
                let p1_voted = stage.p1_vote.is_some();
                let p2_voted = stage.p2_vote.is_some();
                OutgoingMessage::game_player_selection(p1_voted, p2_voted)
            },
            Self::InGame(stage) => OutgoingMessage::GameSync {
                round,
                game: &stage.game,
            },
        }
    }
}

impl From<PlayerSelectionStage> for GameStage {
    fn from(stage: PlayerSelectionStage) -> Self {
        Self::PlayerSelection(stage)
    }
}

impl From<InGameStage> for GameStage {
    fn from(stage: InGameStage) -> Self {
        Self::InGame(stage)
    }
}

pub struct Game {
    stage: GameStage,
    round: u32,
    p1: Addr<actor::Player>,
    p2: Addr<actor::Player>,
}

impl Game {
    #[must_use]
    pub fn new(
        game: Option<InternalGame>,
        config: GameConfig,
        round: u32,
        p1: Addr<actor::Player>,
        p2: Addr<actor::Player>,
    ) -> Self {
        let stage = if let Some(game) = game {
            InGameStage { game }.into()
        } else {
            PlayerSelectionStage::new(config).into()
        };

        Self {
            stage,
            round,
            p1,
            p2,
        }
    }

    fn get_player_addr(&self, player: Player) -> &Addr<actor::Player> {
        match player {
            Player::P1 => &self.p1,
            Player::P2 => &self.p2,
        }
    }

    fn sync(&self) {
        let round = self.round;
        let Ok(sync1) = self.stage.outgoing_message(round).into_shared() else { return };
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

        let Ok(p1_role_msg) = OutgoingMessage::GameRole { role: Player::P1 }.into_serialized() else {
            error!("Failed to serialize game role message, shutting down");
            ctx.stop();
            return;
        };

        let Ok(p2_role_msg) = OutgoingMessage::GameRole { role: Player::P2 }.into_serialized() else {
            error!("Failed to serialize game role message, shutting down");
            ctx.stop();
            return;
        };

        self.p1.do_send(p1_role_msg);
        self.p2.do_send(p2_role_msg);
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

impl Handler<PlayerSelectionVote> for Game {
    type Result = ();

    fn handle(&mut self, msg: PlayerSelectionVote, _: &mut Self::Context) {
        let GameStage::PlayerSelection(stage) = &mut self.stage else {
            return;
        };

        let update_p1 = msg.player == self.p1 && stage.p1_vote.is_none();
        let update_p2 = msg.player == self.p2 && stage.p2_vote.is_none();
        if !(update_p1 || update_p2) {
            return;
        }

        if update_p1 {
            stage.p1_vote = Some(msg.wants_to_start);
        }

        if update_p2 {
            stage.p2_vote = Some(msg.wants_to_start);
        }

        if let PlayerSelectionStage {
            p1_vote: Some(p1_vote),
            p2_vote: Some(p2_vote),
            ..
        } = *stage
        {
            self.stage = InGameStage::new(p1_vote, p2_vote, &stage.config).into();
        }

        self.sync();
    }
}

impl Handler<EndTurn> for Game {
    type Result = ();

    fn handle(&mut self, msg: EndTurn, _: &mut Self::Context) {
        let GameStage::InGame(InGameStage { game }) = &self.stage else {
            return;
        };

        let state = game.state();
        let turn = state.turn;
        let current_player_addr = self.get_player_addr(state.player);
        if !(&msg.player == current_player_addr && turn == msg.turn) {
            return;
        }

        let GameStage::InGame(InGameStage { game }) = &mut self.stage else {
            return;
        };

        if game.end_turn(msg.col).is_ok() {
            self.sync();
        }
    }
}

impl Handler<Restart> for Game {
    type Result = ();

    fn handle(&mut self, _: Restart, _: &mut Self::Context) {
        match &self.stage {
            GameStage::PlayerSelection(stage) => {
                self.stage = PlayerSelectionStage::new(stage.config.clone()).into();
            }
            GameStage::InGame(InGameStage { game }) => {
                let rules = GameConfig::from_game_rules(game.rules());
                self.stage = PlayerSelectionStage::new(rules).into();
            }
        }

        self.round = self.round.wrapping_add(1);
        self.sync();
        debug!("Restarted");
    }
}
