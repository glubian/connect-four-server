use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

use Player::{P1, P2};

pub const FIELD_SIZE: usize = 7;
pub const WIN_LEN: usize = 4;

type GameField = [[Option<Player>; FIELD_SIZE]; FIELD_SIZE];
type GameMatch = ((usize, usize), (usize, usize));

const LAST_MOVE: u32 = (FIELD_SIZE * FIELD_SIZE) as u32 - 1;
const EMPTY_FIELD: GameField = [[None; FIELD_SIZE]; FIELD_SIZE];

#[derive(Serialize, Deserialize)]
pub struct Game {
    field: GameField,
    state: GameState,
    rules: GameRules,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRules {
    pub starting_player: Player,
    pub allow_draws: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr, Debug)]
#[repr(u8)]
pub enum Player {
    P1 = 0,
    P2 = 1,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameState {
    pub player: Player,
    pub turn: u32,
    /// The amount of chips on the field.
    pub moves: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<GameResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_move: Option<usize>,
}

#[derive(Serialize, Deserialize)]
pub struct GameResult {
    pub winner: GameWinner,
    pub matches: Vec<GameMatch>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr, Debug)]
#[repr(u8)]
pub enum GameWinner {
    P1 = P1 as u8,
    P2 = P2 as u8,
    Draw = 2,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EndTurnError {
    IndexOutOfBounds,
    GameOver,
    ColumnFilled,
}

/// Adds horizontal and vertical matches to the vector.
fn get_horizontal_and_vertical_matches(matches: &mut Vec<GameMatch>, field: &GameField) {
    for i in 0..FIELD_SIZE {
        let mut v_len = 0;
        let mut v_last_player = None;

        let mut h_len = 0;
        let mut h_last_player = None;

        for j in 0..FIELD_SIZE {
            let v_player = field[i][j];
            let h_player = field[j][i];

            if v_player == v_last_player && v_player.is_some() {
                v_len += 1;
            } else {
                if v_len >= WIN_LEN {
                    matches.push(((i, j - v_len), (i, j - 1)));
                }
                v_last_player = v_player;
                v_len = v_player.is_some().into();
            }

            if h_player == h_last_player && h_player.is_some() {
                h_len += 1;
            } else {
                if h_len >= WIN_LEN {
                    matches.push(((j - h_len, i), (j - 1, i)));
                }
                h_last_player = h_player;
                h_len = h_player.is_some().into();
            }
        }

        if v_len >= WIN_LEN {
            matches.push(((i, FIELD_SIZE - v_len), (i, FIELD_SIZE - 1)));
        }

        if h_len >= WIN_LEN {
            matches.push(((FIELD_SIZE - h_len, i), (FIELD_SIZE - 1, i)));
        }
    }
}

/// Adds diagonal matches to the vector.
fn get_diagonal_matches(matches: &mut Vec<GameMatch>, field: &GameField) {
    const D: isize = (FIELD_SIZE - WIN_LEN) as isize;
    for d in -D..=D {
        let dx = (-d.min(0)) as usize;
        let dy = d.max(0) as usize;

        let mut last_p1 = None;
        let mut len1 = 0;

        let mut last_p2 = None;
        let mut len2 = 0;

        let b_max = FIELD_SIZE - d.unsigned_abs();
        for b in 0..b_max {
            let p1 = field[b + dx][b + dy];
            let p2 = field[FIELD_SIZE - 1 - b - dx][b + dy];

            if p1 == last_p1 && p1.is_some() {
                len1 += 1;
            } else {
                if len1 >= WIN_LEN {
                    let x1 = b + dx - len1;
                    let y1 = b + dy - len1;
                    let x2 = b + dx - 1;
                    let y2 = b + dy - 1;
                    matches.push(((x1, y1), (x2, y2)));
                }
                last_p1 = p1;
                len1 = p1.is_some().into();
            }

            if p2 == last_p2 && p2.is_some() {
                len2 += 1;
            } else {
                if len2 >= WIN_LEN {
                    let x1 = FIELD_SIZE + len2 - 1 - b - dx;
                    let y1 = b + dy - len2;
                    let x2 = FIELD_SIZE - b - dx;
                    let y2 = b + dy - 1;
                    matches.push(((x1, y1), (x2, y2)));
                }
                last_p2 = p2;
                len2 = p2.is_some().into();
            }
        }

        if len1 >= WIN_LEN {
            let x1 = b_max + dx - len1;
            let y1 = b_max + dy - len1;
            let x2 = b_max + dx - 1;
            let y2 = b_max + dy - 1;
            matches.push(((x1, y1), (x2, y2)));
        }

        if len2 >= WIN_LEN {
            let x1 = FIELD_SIZE + len2 - 1 - b_max - dx;
            let y1 = b_max + dy - len2;
            let x2 = FIELD_SIZE - b_max - dx;
            let y2 = b_max + dy - 1;
            matches.push(((x1, y1), (x2, y2)));
        }
    }
}

#[must_use]
fn get_result(field: &GameField, moves: u32) -> Option<GameResult> {
    let mut matches = Vec::new();

    get_horizontal_and_vertical_matches(&mut matches, field);
    get_diagonal_matches(&mut matches, field);

    if !matches.is_empty() {
        let winner = matches
            .iter()
            .copied()
            .fold((false, false), |(p1, p2), ((x, y), _)| match field[x][y] {
                Some(P1) => (true, p2),
                Some(P2) => (p1, true),
                None => (p1, p2),
            });

        let winner = match winner {
            (true, true) => GameWinner::Draw,
            (true, false) => GameWinner::P1,
            (false, true) => GameWinner::P2,
            (false, false) => return None,
        };

        return Some(GameResult { winner, matches });
    }

    if moves >= LAST_MOVE {
        return Some(GameResult {
            winner: GameWinner::Draw,
            matches: Vec::new(),
        });
    }

    None
}

impl Game {
    #[must_use]
    pub const fn new(rules: GameRules) -> Self {
        Self {
            field: EMPTY_FIELD,
            state: GameState::new(rules.starting_player),
            rules,
        }
    }

    /// Returns true if the most recent move was winning.
    #[must_use]
    fn was_last_move_winning(&self) -> bool {
        let Some(x) = self.state.last_move else {
            return false;
        };
        let other_player = self.state.player.other();
        self.field[x]
            .iter()
            .position(|y| *y == Some(other_player))
            .map_or(false, |y| self.is_move_winning(x, y, other_player))
    }

    /// Returns a result if the current round has resolved the game.
    ///
    /// This function respects `GameRules`. It always runs `is_move_winning()`
    /// before `get_results()`.
    #[must_use]
    fn get_result(&self, point: Option<(usize, usize)>) -> Option<GameResult> {
        let Self {
            field,
            state,
            rules,
        } = &self;
        let player = state.player;
        let moves = state.moves;

        let Some((x, y)) = point else {
            return if rules.allow_draws && player == rules.starting_player.other() && self.was_last_move_winning() {
                match get_result(field, moves) {
                    Some(res) => Some(res),
                    None => unreachable!(),
                }
            } else {
                None
            }
        };

        if moves >= LAST_MOVE {
            return match get_result(field, moves) {
                Some(res) => Some(res),
                None => unreachable!(),
            };
        }

        if rules.allow_draws {
            if player == rules.starting_player {
                return None;
            }

            if self.was_last_move_winning() {
                return match get_result(field, moves) {
                    Some(res) => Some(res),
                    None => unreachable!(),
                };
            }
        }

        if self.is_move_winning(x, y, player) {
            return match get_result(field, moves) {
                Some(res) => Some(res),
                None => unreachable!(),
            };
        }

        None
    }

    /// Ends the current turn.
    ///
    /// Errors:
    ///
    /// - `GameOver` when the game is resolved
    /// - `IndexOutOfBounds` if `col` is outside of `0..FIELD_SIZE` range
    /// - `ColumnFilled` when there no space left in the column
    pub fn end_turn(&mut self, col: Option<usize>) -> Result<(), EndTurnError> {
        if self.state.result.is_some() {
            return Err(EndTurnError::GameOver);
        }

        let Some(col) = col else {
            self.state.result = self.get_result(None);
            self.state.next_turn(None);
            return Ok(());
        };

        if col >= self.field.len() {
            return Err(EndTurnError::IndexOutOfBounds);
        }

        for i in (0..FIELD_SIZE).rev() {
            if self.field[col][i].is_some() {
                continue;
            }

            self.field[col][i] = Some(self.state.player);
            self.state.result = self.get_result(Some((col, i)));
            self.state.next_turn(Some(col));
            return Ok(());
        }

        Err(EndTurnError::ColumnFilled)
    }

    #[must_use]
    fn len_horizontal(&self, x: usize, y: usize, player: Player) -> usize {
        let mut len = 1;

        for x in (0..x).rev() {
            match self.field[x][y] {
                Some(p) if player == p => len += 1,
                _ => break,
            }
        }

        for x in (x + 1)..FIELD_SIZE {
            match self.field[x][y] {
                Some(p) if player == p => len += 1,
                _ => break,
            }
        }

        len
    }

    #[must_use]
    fn len_vertical(&self, x: usize, y: usize, player: Player) -> usize {
        let mut len = 1;

        for y in (0..y).rev() {
            match self.field[x][y] {
                Some(p) if player == p => len += 1,
                _ => break,
            }
        }

        for y in (y + 1)..FIELD_SIZE {
            match self.field[x][y] {
                Some(p) if player == p => len += 1,
                _ => break,
            }
        }

        len
    }

    #[must_use]
    fn len_diagonal_tl_br(&self, x: usize, y: usize, player: Player) -> usize {
        let mut len = 1;
        for d in 1..=(x.min(y)) {
            match self.field[x - d][y - d] {
                Some(p) if player == p => len += 1,
                _ => break,
            }
        }

        for d in 1..(FIELD_SIZE - x.max(y)) {
            match self.field[x + d][y + d] {
                Some(p) if player == p => len += 1,
                _ => break,
            }
        }

        len
    }

    #[must_use]
    fn len_diagonal_tr_bl(&self, x: usize, y: usize, player: Player) -> usize {
        let mut len = 1;

        for d in 1..=(y.min(FIELD_SIZE - 1 - x)) {
            match self.field[x + d][y - d] {
                Some(p) if player == p => len += 1,
                _ => break,
            }
        }

        for d in 1..=(x.min(FIELD_SIZE - 1 - y)) {
            match self.field[x - d][y + d] {
                Some(p) if player == p => len += 1,
                _ => break,
            }
        }

        len
    }

    #[must_use]
    fn is_move_winning(&self, x: usize, y: usize, player: Player) -> bool {
        self.len_horizontal(x, y, player) >= WIN_LEN
            || self.len_vertical(x, y, player) >= WIN_LEN
            || self.len_diagonal_tl_br(x, y, player) >= WIN_LEN
            || self.len_diagonal_tr_bl(x, y, player) >= WIN_LEN
    }

    #[must_use]
    pub fn field(&self) -> &GameField {
        &self.field
    }

    #[must_use]
    pub fn rules(&self) -> &GameRules {
        &self.rules
    }

    #[must_use]
    pub fn state(&self) -> &GameState {
        &self.state
    }
}

impl GameState {
    #[must_use]
    const fn new(starting_player: Player) -> Self {
        Self {
            player: starting_player,
            turn: 0,
            moves: 0,
            result: None,
            last_move: None,
        }
    }

    fn next_turn(&mut self, col: Option<usize>) {
        self.turn += 1;
        if col.is_some() {
            self.moves += 1;
        }
        self.player = self.player.other();
        self.last_move = col;
    }
}

impl Player {
    #[must_use]
    pub const fn other(&self) -> Self {
        match self {
            Self::P1 => Self::P2,
            Self::P2 => Self::P1,
        }
    }
}

impl Default for Game {
    fn default() -> Self {
        Self::new(GameRules::default())
    }
}

impl Default for GameRules {
    fn default() -> Self {
        Self {
            starting_player: P1,
            allow_draws: false,
        }
    }
}

impl From<Player> for GameWinner {
    fn from(player: Player) -> Self {
        match player {
            P1 => Self::P1,
            P2 => Self::P2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fast_forward_game(rules: GameRules, moves: &[usize]) -> Game {
        let mut game = Game::new(rules);
        for i in moves.iter().map(|i| Some(i - 1)) {
            game.end_turn(i).unwrap();
        }

        game
    }

    fn won_game_horizontal(rules: GameRules) -> Game {
        fast_forward_game(rules, &[4, 4, 5, 5, 6, 6, 7])
    }

    fn won_game_vertical(rules: GameRules) -> Game {
        fast_forward_game(rules, &[4, 5, 4, 5, 4, 5, 4])
    }

    fn won_game_diagonal1(rules: GameRules) -> Game {
        fast_forward_game(rules, &[4, 5, 5, 7, 6, 6, 6, 6, 7, 7, 7])
    }

    fn won_game_diagonal2(rules: GameRules) -> Game {
        fast_forward_game(rules, &[4, 3, 3, 1, 2, 2, 2, 1, 1, 5, 1])
    }

    fn won_game_1(rules: GameRules) -> Game {
        let moves = [1, 2, 3, 4, 1, 2, 3, 4, 5, 5, 2, 2, 3, 4, 4, 2, 1, 3];
        fast_forward_game(rules, &moves)
    }

    fn won_game_2(rules: GameRules) -> Game {
        let moves = [3, 4, 4, 4, 5, 5, 5, 6, 6, 6, 6];
        fast_forward_game(rules, &moves)
    }

    fn won_game_3(rules: GameRules) -> Game {
        let moves = [4, 5, 1, 7, 3, 6, 2];
        fast_forward_game(rules, &moves)
    }

    fn drawn_game(rules: GameRules) -> (Game, Result<(), ()>) {
        let mut game = won_game_horizontal(rules);
        let drawn = game.end_turn(Some(6)).map_err(|_| ());
        (game, drawn)
    }

    fn filled_game(rules: GameRules) -> Game {
        let moves = [
            1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 2, 3, 4, 5, 6, 7, 2, 5,
            6, 7, 7, 1, 1, 2, 3, 4, 5, 6, 1, 1, 2, 3, 4, 5, 6, 3, 4, 7,
        ];
        fast_forward_game(rules, &moves)
    }

    #[test]
    fn game_end_turn_success() {
        let rules = GameRules::default();
        let mut game = Game::new(rules);
        game.end_turn(Some(3)).unwrap();
        assert_eq!(game.state.turn, 1);
        assert_eq!(game.state.player, P2);
    }

    #[test]
    fn game_end_turn_out_of_bounds() {
        let rules = GameRules::default();
        let mut game = Game::new(rules);
        assert_eq!(game.end_turn(Some(7)), Err(EndTurnError::IndexOutOfBounds));
    }

    #[test]
    fn game_over_end_turn() {
        let rules = GameRules::default();
        let mut game = won_game_horizontal(rules);
        assert_eq!(game.end_turn(Some(2)), Err(EndTurnError::GameOver));
    }

    #[test]
    fn game_end_turn_column_filled() {
        let rules = GameRules::default();
        let mut game = Game::new(rules);
        for _ in 0..FIELD_SIZE {
            game.end_turn(Some(3)).unwrap();
        }
        assert_eq!(game.end_turn(Some(3)), Err(EndTurnError::ColumnFilled));
    }

    #[test]
    fn is_game_over_horizontal() {
        let game = won_game_horizontal(GameRules::default());
        assert!(game.state.result.is_some());
    }

    #[test]
    fn is_game_over_vertical() {
        let game = won_game_vertical(GameRules::default());
        assert!(game.state.result.is_some());
    }

    #[test]
    fn is_game_over_diagonal1() {
        let game = won_game_diagonal1(GameRules::default());
        assert!(game.state.result.is_some());
    }

    #[test]
    fn is_game_over_diagonal2() {
        let game = won_game_diagonal2(GameRules::default());
        assert!(game.state.result.is_some());
    }

    #[test]
    fn is_game_over_1() {
        let game = won_game_1(GameRules::default());
        assert!(game.state.result.is_some());
    }

    #[test]
    fn is_game_over_2() {
        let game = won_game_2(GameRules::default());
        assert!(game.state.result.is_some());
    }

    #[test]
    fn is_game_over_3() {
        let game = won_game_3(GameRules::default());
        assert!(game.state.result.is_some());
    }

    #[test]
    fn rule_disallow_draws() {
        for starting_player in [P1, P2] {
            let rules = GameRules {
                starting_player,
                allow_draws: false,
            };
            let (game, res) = drawn_game(rules);
            assert!(res.is_err());
            assert_eq!(game.state.result.map(|r| r.winner), Some(starting_player.into()));
        }
    }

    #[test]
    fn rule_allow_draws() {
        for starting_player in [P1, P2] {
            let rules = GameRules {
                starting_player,
                allow_draws: true,
            };
            let (game, res) = drawn_game(rules);
            assert!(res.is_ok());
            assert_eq!(game.state.result.map(|r| r.winner), Some(GameWinner::Draw));
        }
    }

    #[test]
    fn is_game_over_incremental_in_game() {
        let player = P1;
        let rules = GameRules::default();
        let mut game = Game::new(rules);
        game.end_turn(Some(3)).unwrap();
        assert!(!game.is_move_winning(3, 6, player));
    }

    #[test]
    fn is_game_over_incremental_horizontal() {
        let game = won_game_horizontal(GameRules::default());
        assert!(game.is_move_winning(6, 6, P1));
    }

    #[test]
    fn is_game_over_incremental_vertical() {
        let game = won_game_vertical(GameRules::default());
        assert!(game.is_move_winning(3, 3, P1));
    }

    #[test]
    fn is_game_over_incremental_diagonal1() {
        let game = won_game_diagonal1(GameRules::default());
        assert!(game.is_move_winning(6, 3, P1));
        assert!(game.is_move_winning(5, 4, P1));
        assert!(game.is_move_winning(4, 5, P1));
        assert!(game.is_move_winning(3, 6, P1));
    }

    #[test]
    fn is_game_over_incremental_diagonal2() {
        let game = won_game_diagonal2(GameRules::default());
        assert!(game.is_move_winning(0, 3, P1));
        assert!(game.is_move_winning(1, 4, P1));
        assert!(game.is_move_winning(2, 5, P1));
        assert!(game.is_move_winning(3, 6, P1));
    }

    #[test]
    fn is_game_over_incremental_1() {
        let game = won_game_1(GameRules::default());
        assert!(game.is_move_winning(4, 5, P2));
        assert!(game.is_move_winning(3, 4, P2));
        assert!(game.is_move_winning(2, 3, P2));
        assert!(game.is_move_winning(1, 2, P2));
    }

    #[test]
    fn is_game_over_incremental_2() {
        let game = won_game_2(GameRules::default());
        assert!(game.is_move_winning(2, 6, P1));
        assert!(game.is_move_winning(3, 5, P1));
        assert!(game.is_move_winning(4, 4, P1));
        assert!(game.is_move_winning(5, 3, P1));
    }

    #[test]
    fn is_game_over_when_filled() {
        let game = filled_game(GameRules::default());
        assert_eq!(game.state.turn, 49);
        assert!(game.state.result.is_some());
    }
}
