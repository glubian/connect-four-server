use serde::{Deserialize, Serialize};

use crate::{
    game::GameRules,
    wrap::{OptionalFields, RequiredFields, Wrap},
};

/// Internally used to generate different versions of `GameConfig`
/// with fields wrapped in different types.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GenGameConfig<W: Wrap> {
    pub allow_draws: W::Wrapped<bool>,
}

/// A subset of `GameRules` used for starting a new game.
pub type GameConfig = GenGameConfig<RequiredFields>;
/// A subset of `GameRules` used for starting a new game. All fields are optional.
pub type PartialGameConfig = GenGameConfig<OptionalFields>;

impl GameConfig {
    /// Create a new `GameConfig` with values copied from `PartialGameConfig`,
    /// where possible. If a value is missing, default value will be used instead.
    #[must_use]
    pub fn from_partial(partial: &PartialGameConfig) -> Self {
        Self {
            allow_draws: partial.allow_draws.unwrap_or_default(),
        }
    }

    /// Create a new `GameConfig` with values copied from `GameRules`.
    #[must_use]
    pub fn from_game_rules(rules: &GameRules) -> Self {
        Self {
            allow_draws: rules.allow_draws,
        }
    }

    /// Overwrites any settings contained within a `PartialGameConfig`.
    pub fn apply_partial(&mut self, partial: &PartialGameConfig) {
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
            allow_draws: Some(config.allow_draws),
        }
    }
}

impl From<GameConfig> for PartialGameConfig {
    fn from(config: GameConfig) -> Self {
        Self::from_full(&config)
    }
}
