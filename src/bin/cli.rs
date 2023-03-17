#![warn(clippy::all, clippy::pedantic)]

use connect_four_server::game::{EndTurnError, Game, GameRules, GameWinner, Player, FIELD_SIZE};
use std::io::BufRead;

struct App {
    game: Game,
    moves: Vec<usize>,
}

impl App {
    fn new() -> Self {
        Self {
            game: Game::new(GameRules::default()),
            moves: Vec::new(),
        }
    }

    fn run(&mut self) {
        print!("{}", self.game.to_string());

        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else {
                continue;
            };

            line.split(", ").for_each(|cmd| self.process_move(cmd));
            print!("{}", self.game.to_string());
        }
    }

    fn restart(&mut self) {
        self.game = Game::new(self.game.rules().clone());
        self.moves.clear();
    }

    fn process_move(&mut self, cmd: &str) {
        if cmd.chars().count() == 1 {
            let c = cmd.chars().next().unwrap();
            if !('1'..='7').contains(&c) {
                return;
            }

            let m = (c as usize) - ('1' as usize);
            let res = self.game.end_turn(m);
            if let Err(EndTurnError::GameOver) = res {
                println!("Game over!");
                return;
            } else if let Err(EndTurnError::ColumnFilled) = res {
                println!("Not enough space!");
                return;
            };

            self.moves.push(m);
            return;
        }

        match cmd {
            "restart" => self.restart(),
            "moves" => {
                let moves = self
                    .moves
                    .iter()
                    .copied()
                    .map(|m| m + 1)
                    .collect::<Vec<usize>>();
                println!("{moves:?}");
            }
            "json" => println!("{}", serde_json::to_string_pretty(&self.game).unwrap()),
            "exit" => std::process::exit(0),
            _ => (),
        }
    }
}

fn main() {
    App::new().run();
}

trait ToString {
    fn to_string(&self) -> String;
}

impl ToString for Game {
    fn to_string(&self) -> String {
        let mut res = String::with_capacity(1024);

        res.push_str(&"-".repeat(14));
        res.push('\n');

        let player = match &self.state().result {
            Some(res) => match res.winner {
                GameWinner::P1 => "(Player 1) has won!",
                GameWinner::P2 => "[Player 2] has won!",
                GameWinner::Draw => "It's a draw!",
            },
            None => match self.state().player {
                Player::P1 => "(Player 1)'s turn",
                Player::P2 => "[Player 2]'s turn",
            },
        };

        res.push_str(player);
        res.push('\n');

        for y in 0..FIELD_SIZE {
            for x in 0..FIELD_SIZE {
                match self.field()[x][y] {
                    Some(Player::P1) => res.push_str("()"),
                    Some(Player::P2) => res.push_str("[]"),
                    None => res.push_str("  "),
                }
            }

            res.push('\n');
        }

        res.push_str("1 2 3 4 5 6 7\n");
        res
    }
}
