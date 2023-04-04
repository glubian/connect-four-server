use std::ops::{Index, IndexMut};

use crate::game::Player;

/// Stores one type T per player. Can be accessed by passing `Player` as index.
pub struct PlayerTuple<T>([T; 2]);

impl<T> PlayerTuple<T> {
    #[must_use]
    pub const fn new(tuple: [T; 2]) -> Self {
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
