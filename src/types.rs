use strum::Display;

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, Display)]
pub enum Time {
    NONE,
    MISC,
    BULLET,
    BLITZ,
    RAPID,
    DAILY,
}
impl Time {
    pub fn parse(val: &str) -> Time {
        let seconds = match val.split('+').next() {
            Some(s) => match s.parse::<i32>() {
                Ok(s) => s,
                Err(_) => return Time::MISC,
            },
            None => return Time::MISC,
        };

        if seconds <= 120 {
            Self::BULLET
        } else if seconds <= 600 {
            Self::BLITZ
        } else if seconds <= 1500 {
            Self::RAPID
        } else {
            Self::MISC
        }
    }
}
impl Default for Time {
    fn default() -> Self {
        Time::NONE
    }
}

#[derive(Default, Debug)]
pub struct Game {
    pub pgn: String,
    pub time: Time,
    pub white: String,
    pub black: String,
}

#[derive(Hash, PartialEq, Eq, Display)]
pub enum Color {
    NONE,
    WHITE,
    BLACK,
}
#[derive(Hash, PartialEq, Eq)]
pub struct PGNMetadata {
    pub username: String,
    pub color: Color,
    pub time: Time,
}

impl PGNMetadata {
    pub fn from_game(username: &String, game: &Game, ignore_time: bool) -> PGNMetadata {
        PGNMetadata {
            username: username.clone(),
            color: if *username == game.white {
                Color::WHITE
            } else {
                Color::BLACK
            },
            time: if ignore_time { Time::NONE } else { game.time },
        }
    }
    pub fn from_username(username: &String) -> PGNMetadata {
        PGNMetadata {
            username: username.clone(),
            color: Color::NONE,
            time: Time::NONE,
        }
    }
}

impl std::fmt::Display for PGNMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut r = write!(f, "{}", self.username);
        if self.color != Color::NONE {
            r = r.and(write!(f, "_{}", self.color))
        }
        if self.time != Time::NONE {
            r = r.and(write!(f, "_{}", self.time))
        }
        r.and(write!(f, ".pgn"))
    }
}
