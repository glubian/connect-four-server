#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::wildcard_imports)]
// TODO: Remove this and add documentation
#![allow(clippy::missing_errors_doc)]

pub mod game;
pub mod game_config;
pub mod server;
