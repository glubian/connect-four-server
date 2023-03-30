use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::server::serde::{as_secs, as_secs_optional};

/// A subset of `GameRules` used for starting a new game.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GameConfig {
    #[serde(with = "as_secs")]
    pub time_per_turn: Duration,
    #[serde(with = "as_secs")]
    pub time_cap: Duration,
    pub allow_draws: bool,
}

/// A subset of `GameRules` used for starting a new game. All fields are optional.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PartialGameConfig {
    #[serde(with = "as_secs_optional", skip_serializing_if = "Option::is_none")]
    pub time_per_turn: Option<Duration>,
    #[serde(with = "as_secs_optional", skip_serializing_if = "Option::is_none")]
    pub time_cap: Option<Duration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_draws: Option<bool>,
}

impl GameConfig {
    /// Create a new `GameConfig` with values copied from `PartialGameConfig`,
    /// where possible. If a value is missing, default value will be used instead.
    #[must_use]
    pub fn from_partial(partial: &PartialGameConfig) -> Self {
        Self {
            time_per_turn: partial.time_per_turn.unwrap_or_default(),
            time_cap: partial.time_cap.unwrap_or_default(),
            allow_draws: partial.allow_draws.unwrap_or_default(),
        }
    }

    /// Overwrites any settings contained within a `PartialGameConfig`.
    pub fn apply_partial(&mut self, partial: &PartialGameConfig) {
        if let Some(time_per_turn) = partial.time_per_turn {
            self.time_per_turn = time_per_turn;
        }

        if let Some(time_cap) = partial.time_cap {
            self.time_cap = time_cap;
        }

        if let Some(allow_draws) = partial.allow_draws {
            self.allow_draws = allow_draws;
        }
    }
}

impl From<PartialGameConfig> for GameConfig {
    fn from(partial: PartialGameConfig) -> Self {
        Self::from_partial(&partial)
    }
}

impl PartialGameConfig {
    /// Create a new `PartialGameConfig` with values copied from `GameConfig`.
    #[must_use]
    fn from_full(config: &GameConfig) -> Self {
        Self {
            time_per_turn: Some(config.time_per_turn),
            time_cap: Some(config.time_cap),
            allow_draws: Some(config.allow_draws),
        }
    }
}

impl From<GameConfig> for PartialGameConfig {
    fn from(config: GameConfig) -> Self {
        Self::from_full(&config)
    }
}

impl PartialEq for GameConfig {
    fn eq(&self, other: &Self) -> bool {
        self.time_per_turn == other.time_per_turn
            && self.time_cap == other.time_cap
            && self.allow_draws == other.allow_draws
    }
}
