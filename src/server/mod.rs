pub mod actor;
pub mod cli;
pub mod config;
mod game_config;
mod player_tuple;
pub mod serde;

pub use cli::AppArgs;
pub use config::AppConfig;
use game_config::{GameConfig, PartialGameConfig};
pub use player_tuple::PlayerTuple;
