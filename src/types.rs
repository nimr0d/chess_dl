use strum::Display;

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, Display)]
pub enum Time {
    BULLET = 1,
    BLITZ = 2,
    RAPID = 3,
    DAILY = 4,
    ALL = 5,
}
impl Time {
    pub fn parse(val: &str) -> Option<Time> {
        let seconds = val.split('+').next()?.parse::<i32>().ok()?;
        if seconds <= 120 {
            Some(Self::BULLET)
        } else if seconds <= 600 {
            Some(Self::BLITZ)
        } else {
            Some(Self::RAPID)
        }
    }
}

#[derive(Default, Debug)]
pub struct Game {
    pub pgn: String,
    pub time: Option<Time>,
    pub white: String,
    pub black: String,
}

#[derive(Hash, PartialEq, Eq, Display)]
pub enum Color {
    WHITE,
    BLACK,
}
#[derive(Hash, PartialEq, Eq)]
pub struct GameInfo {
    pub username: String,
    pub color: Color,
    pub time: Option<Time>,
}

impl GameInfo {
    pub fn from_game(username: &String, game: &Game) -> GameInfo {
        if *username == game.white {
            GameInfo {
                username: username.clone(),
                color: Color::WHITE,
                time: game.time,
            }
        } else {
            GameInfo {
                username: username.clone(),
                color: Color::BLACK,
                time: game.time,
            }
        }
    }
}
