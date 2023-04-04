use std::ops::{Index, IndexMut};
use std::sync::Arc;
use std::time::{Duration, Instant};

use actix::prelude::*;
use actix_web::Either;
use chrono::{DateTime, Utc};
use log::debug;
use rand::Rng;

use crate::game::Game as InternalGame;
use crate::game::{
    GameRules,
    Player::{self, P1, P2},
};

use crate::game_config::{GameConfig, PartialGameConfig};
use crate::server::constants::TIME_PER_TURN_MIN;
use crate::server::{actor, AppConfig};
use actor::player::{self, AttachController, Disconnect, Disconnected, OutgoingMessage};

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
    pub col: Option<usize>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Restart {
    pub addr: Addr<actor::Player>,
    pub partial: Option<PartialGameConfig>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RestartResponse {
    pub addr: Addr<actor::Player>,
    pub accepted: bool,
}

struct PlayerSelectionStage {
    p1_vote: Option<bool>,
    p2_vote: Option<bool>,
}

impl PlayerSelectionStage {
    fn new() -> Self {
        Self {
            p1_vote: None,
            p2_vote: None,
        }
    }
}

struct InGameStage {
    game: InternalGame,
    extra_time: PlayerTuple<Duration>,
    timeout: Option<TurnTimeout>,
}

struct TurnTimeout {
    handle: SpawnHandle,
    chrono: DateTime<Utc>,
    instant: Instant,
}

impl InGameStage {
    fn starting_player(p1_vote: bool, p2_vote: bool) -> Player {
        if p1_vote && !p2_vote {
            P1
        } else if p2_vote && !p1_vote {
            P2
        } else if rand::thread_rng().gen::<bool>() {
            P1
        } else {
            P2
        }
    }

    fn new(p1_vote: bool, p2_vote: bool, rules: &GameConfig) -> Self {
        let starting_player = Self::starting_player(p1_vote, p2_vote);
        let rules = GameRules {
            starting_player,
            allow_draws: rules.allow_draws,
        };
        let game = InternalGame::new(rules);
        Self {
            game,
            extra_time: PlayerTuple::new([Duration::ZERO, Duration::ZERO]),
            timeout: None,
        }
    }
}

impl From<InternalGame> for InGameStage {
    fn from(g: InternalGame) -> Self {
        Self {
            game: g,
            extra_time: PlayerTuple::new([Duration::ZERO, Duration::ZERO]),
            timeout: None,
        }
    }
}

enum GameStage {
    PlayerSelection(PlayerSelectionStage),
    InGame(InGameStage),
}

impl GameStage {
    fn is_game_over(&self) -> bool {
        if let Self::InGame(InGameStage { game, .. }) = self {
            game.state().result.is_some()
        } else {
            false
        }
    }

    fn outgoing_message(&self, round: u32) -> OutgoingMessage {
        match self {
            Self::PlayerSelection(stage) => {
                let p1_voted = stage.p1_vote.is_some();
                let p2_voted = stage.p2_vote.is_some();
                OutgoingMessage::game_player_selection(p1_voted, p2_voted)
            }
            Self::InGame(stage) => {
                let game = &stage.game;
                let timeout = stage.timeout.as_ref().map(|t| t.chrono);
                OutgoingMessage::game_sync(round, game, timeout)
            }
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

/// Restart request with optional changes to the config.
struct RestartRequest {
    /// Changed config.
    config: Option<GameConfig>,
    /// Timeout handle.
    handle: SpawnHandle,
    /// Timeout timestamp.
    timestamp: DateTime<Utc>,
}

impl RestartRequest {
    fn to_outgoing(&self) -> player::RestartRequest {
        player::RestartRequest::new(self.config.as_ref(), self.timestamp)
    }
}

/// Stores one type T per player. Can be accessed by passing `Player` as index.
struct PlayerTuple<T>([T; 2]);

impl<T> PlayerTuple<T> {
    #[must_use]
    const fn new(tuple: [T; 2]) -> Self {
        Self(tuple)
    }
}

impl<T> From<[T; 2]> for PlayerTuple<T> {
    fn from(tuple: [T; 2]) -> Self {
        Self(tuple)
    }
}

impl<T> Index<Player> for PlayerTuple<T> {
    type Output = T;

    fn index(&self, player: Player) -> &Self::Output {
        &self.0[player as usize]
    }
}

impl<T> IndexMut<Player> for PlayerTuple<T> {
    fn index_mut(&mut self, player: Player) -> &mut Self::Output {
        &mut self.0[player as usize]
    }
}

pub struct Game {
    stage: GameStage,
    round: u32,
    config: GameConfig,
    addrs: PlayerTuple<Addr<actor::Player>>,
    restart_requests: PlayerTuple<Option<RestartRequest>>,
    cfg: Arc<AppConfig>,
}

impl Game {
    #[must_use]
    pub fn new(
        game: Option<InternalGame>,
        config: GameConfig,
        round: u32,
        extra_time: Option<[Duration; 2]>,
        p1: Addr<actor::Player>,
        p2: Addr<actor::Player>,
        cfg: Arc<AppConfig>,
    ) -> Self {
        let stage: GameStage = if let Some(game) = game {
            InGameStage {
                game,
                extra_time: extra_time.unwrap_or_default().into(),
                timeout: None,
            }
            .into()
        } else {
            PlayerSelectionStage::new().into()
        };

        Self {
            stage,
            round,
            config,
            addrs: PlayerTuple::new([p1, p2]),
            restart_requests: PlayerTuple::new([None, None]),
            cfg,
        }
    }

    /// Returns which player the address belongs to, or None if the address
    /// does not belong to either player in this instance.
    fn get_player(&self, player_addr: &Addr<actor::Player>) -> Option<Player> {
        if &self.addrs[P1] == player_addr {
            Some(P1)
        } else if &self.addrs[P2] == player_addr {
            Some(P2)
        } else {
            None
        }
    }

    fn sync(&self) {
        let round = self.round;
        let sync1 = self.stage.outgoing_message(round).into_shared().unwrap();
        let sync2 = sync1.clone();
        self.addrs[P1].do_send(sync1);
        self.addrs[P2].do_send(sync2);
    }

    /// Sends `OutgoingMessage::GameRestartRequest` to both players.
    fn sync_restart_request(&self, player: Player) {
        let req = &self.restart_requests[player];
        let player_req = req.as_ref().map(RestartRequest::to_outgoing);
        let msg1 = OutgoingMessage::game_restart_request(player, player_req)
            .into_shared()
            .unwrap();
        let msg2 = msg1.clone();
        self.addrs[P1].do_send(msg1);
        self.addrs[P2].do_send(msg2);
    }

    /// Sends `OutgoingMessage::GameSetup` containing the current configuration.
    fn sync_config(&self) {
        let msg: OutgoingMessage = OutgoingMessage::game_setup()
            .set_config(&self.config)
            .into();
        let msg1 = msg.into_shared().unwrap();
        let msg2 = msg1.clone();
        self.addrs[P1].do_send(msg1);
        self.addrs[P2].do_send(msg2);
    }

    /// Applies configuration from the restart request.
    fn accept_restart_request(&mut self, player: Player, ctx: &mut Context<Self>) {
        let Some(req) = self.restart_requests[player].take() else { return };
        self.dismiss_duplicate_restart_requests(ctx);
        ctx.cancel_future(req.handle);
        if let Some(config) = req.config {
            self.config = config;
            self.sync_config();
        }
        self.sync_restart_request(player);
    }

    /// Rejects the request to restart the game.
    fn reject_restart_request(&mut self, player: Player, ctx: &mut Context<Self>) {
        let Some(req) = self.restart_requests[player].take() else { return };
        ctx.cancel_future(req.handle);
        self.sync_restart_request(player);
    }

    /// Deletes the restart request made by player 1.
    fn on_p1_request_timeout(&mut self, _: &mut Context<Self>) {
        self.restart_requests[P1].take();
        self.sync_restart_request(P1);
    }

    /// Deletes the restart request made by player 2.
    fn on_p2_request_timeout(&mut self, _: &mut Context<Self>) {
        self.restart_requests[P2].take();
        self.sync_restart_request(P2);
    }

    /// Creates a new restart request.
    fn create_restart_request(
        duration: Duration,
        player: Player,
        config: Option<GameConfig>,
        ctx: &mut Context<Self>,
    ) -> RestartRequest {
        let handle = match player {
            P1 => ctx.run_later(duration, Self::on_p1_request_timeout),
            P2 => ctx.run_later(duration, Self::on_p2_request_timeout),
        };
        let timeout =
            chrono::Duration::from_std(duration).unwrap_or_else(|_| chrono::Duration::zero());
        let timestamp = Utc::now() + timeout;
        RestartRequest {
            config,
            handle,
            timestamp,
        }
    }

    /// Dismisses restart requests that do not change the current config.
    fn dismiss_duplicate_restart_requests(&mut self, ctx: &mut Context<Self>) {
        for player in [P1, P2] {
            let Some(req) = self.restart_requests[player].as_ref() else { continue };
            let req_config = req.config.as_ref();
            if req_config.map_or(false, |c| c == &self.config) {
                let req = self.restart_requests[player].take().unwrap();
                ctx.cancel_future(req.handle);
                self.sync_restart_request(player);
            }
        }
    }

    /// Dismisses the previous request and creates a new one.
    fn update_restart_request(
        &mut self,
        config: Option<GameConfig>,
        player: Player,
        ctx: &mut Context<Self>,
    ) {
        if let Some(req) = self.restart_requests[player].take() {
            ctx.cancel_future(req.handle);
        }
        self.restart_requests[player] = Some(Self::create_restart_request(
            self.cfg.restart_request_timeout,
            player,
            config,
            ctx,
        ));
        self.sync_restart_request(player);
    }

    /// Called when the time has ran out.
    fn on_timeout(&mut self, ctx: &mut Context<Self>) {
        let GameStage::InGame(InGameStage { game, .. }) = &self.stage else {
            return;
        };
        let msg = EndTurn {
            col: None,
            player: Addr::clone(&self.addrs[game.state().player]),
            turn: game.state().turn,
        };
        Self::handle(self, msg, ctx);
    }

    /// Returns the amount of time the current turn should take, or `0`
    /// if timer is disabled.
    fn get_timeout_duration(extra_time: Duration, config: &GameConfig) -> Duration {
        let GameConfig {
            time_per_turn,
            time_cap,
            ..
        } = *config;

        if time_per_turn < TIME_PER_TURN_MIN {
            return Duration::ZERO;
        }

        let time_cap = time_cap.max(time_per_turn);
        (extra_time + time_per_turn).min(time_cap)
    }

    /// Starts a timeout, if there is none.
    fn start_timeout(
        timeout: &mut Option<TurnTimeout>,
        duration: Duration,
        ctx: &mut Context<Self>,
    ) {
        if timeout.is_some() || duration < TIME_PER_TURN_MIN {
            return;
        }

        let handle = ctx.run_later(duration, Self::on_timeout);
        let duration_chrono =
            chrono::Duration::from_std(duration).unwrap_or_else(|_| chrono::Duration::zero());

        timeout.replace(TurnTimeout {
            handle,
            chrono: Utc::now() + duration_chrono,
            instant: Instant::now() + duration,
        });
    }

    /// Clears timeout and returns how much time remained until it would fire.
    fn clear_timeout(timeout: &mut Option<TurnTimeout>, ctx: &mut Context<Self>) -> Duration {
        let Some(timeout) = timeout.take() else {
            return Duration::ZERO;
        };

        ctx.cancel_future(timeout.handle);
        timeout.instant - Instant::now()
    }

    /// Restarts the game.
    fn restart(&mut self, ctx: &mut Context<Self>) {
        if let GameStage::InGame(InGameStage { timeout, .. }) = &mut self.stage {
            Self::clear_timeout(timeout, ctx);
        };
        self.dismiss_duplicate_restart_requests(ctx);
        self.stage = PlayerSelectionStage::new().into();
        self.round = self.round.wrapping_add(1);
        self.sync();
        debug!("Restarted");
    }
}

impl Actor for Game {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let res1 = self.addrs[P1].try_send(AttachController(Either::Right(ctx.address())));
        let res2 = self.addrs[P2].try_send(AttachController(Either::Right(ctx.address())));
        if res1.is_err() || res2.is_err() {
            // both controller must be registered successfully in order for WsGame to work properly
            debug!("Failed to attach controller, shutting down");
            ctx.stop();
            return;
        }

        let p1_role_msg = OutgoingMessage::full_game_setup(P1, &self.config)
            .into_serialized()
            .unwrap();
        let p2_role_msg = OutgoingMessage::full_game_setup(P2, &self.config)
            .into_serialized()
            .unwrap();
        self.addrs[P1].do_send(p1_role_msg);
        self.addrs[P2].do_send(p2_role_msg);
        self.sync();
        debug!("Started");
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        debug!("Shutting down");
        self.addrs[P1].do_send(Disconnect::GameEnded);
        self.addrs[P2].do_send(Disconnect::GameEnded);
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

        let update_p1 = msg.player == self.addrs[P1] && stage.p1_vote.is_none();
        let update_p2 = msg.player == self.addrs[P2] && stage.p2_vote.is_none();
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
            self.stage = InGameStage::new(p1_vote, p2_vote, &self.config).into();
        }

        self.sync();
    }
}

impl Handler<EndTurn> for Game {
    type Result = ();

    fn handle(&mut self, msg: EndTurn, ctx: &mut Self::Context) {
        let GameStage::InGame(InGameStage { game, .. }) = &self.stage else {
            return;
        };

        let state = game.state();
        let player = state.player;
        let turn = state.turn;
        if !(msg.player == self.addrs[player] && turn == msg.turn) {
            return;
        }

        let GameStage::InGame(InGameStage {
            game,
            extra_time,
            timeout
        }) = &mut self.stage else {
            return;
        };

        if game.end_turn(msg.col).is_err() {
            return;
        }

        let time_remaining = Self::clear_timeout(timeout, ctx);
        if turn != 0 {
            extra_time[player] = time_remaining;
        }
        if game.state().result.is_none() {
            let extra_time = extra_time[game.state().player];
            let duration = Self::get_timeout_duration(extra_time, &self.config);
            Self::start_timeout(timeout, duration, ctx);
        }
        self.sync();
    }
}

impl Handler<Restart> for Game {
    type Result = ();

    fn handle(&mut self, Restart { addr, partial }: Restart, ctx: &mut Self::Context) {
        let player = self.get_player(&addr).unwrap();
        if let Some(partial) = partial {
            let mut config = self.config.clone();
            config.apply_partial(&partial);
            if self.config == config {
                if self.stage.is_game_over() {
                    self.restart(ctx);
                } else {
                    self.update_restart_request(None, player, ctx);
                }
            } else {
                self.update_restart_request(Some(config), player, ctx);
            }
        } else if self.stage.is_game_over() {
            self.restart(ctx);
        } else {
            self.update_restart_request(None, player, ctx);
        }
    }
}

impl Handler<RestartResponse> for Game {
    type Result = ();

    fn handle(&mut self, msg: RestartResponse, ctx: &mut Self::Context) {
        let opponent = self.get_player(&msg.addr).unwrap().other();
        if msg.accepted {
            self.accept_restart_request(opponent, ctx);
            self.restart(ctx);
        } else {
            self.reject_restart_request(opponent, ctx);
        }
    }
}
