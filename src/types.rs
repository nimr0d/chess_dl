use strum::Display;

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, Display)]
pub enum Time {
    None,
    Misc,
    Bullet,
    Blitz,
    Rapid,
    #[allow(dead_code)]
    Daily,
}
impl Time {
    pub fn parse(val: &str) -> Time {
        let seconds = match val.split('+').next() {
            Some(s) => match s.parse::<i32>() {
                Ok(s) => s,
                Err(_) => return Time::Misc,
            },
            None => return Time::Misc,
        };

        if seconds <= 120 {
            Self::Bullet
        } else if seconds <= 600 {
            Self::Blitz
        } else if seconds <= 1500 {
            Self::Rapid
        } else {
            Self::Misc
        }
    }
}
impl Default for Time {
    fn default() -> Self {
        Time::None
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
    None,
    White,
    Black,
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
                Color::White
            } else {
                Color::Black
            },
            time: if ignore_time { Time::None } else { game.time },
        }
    }
    pub fn from_username(username: &str) -> PGNMetadata {
        PGNMetadata {
            username: String::from(username),
            color: Color::None,
            time: Time::None,
        }
    }
}

impl std::fmt::Display for PGNMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut r = write!(f, "{}", self.username);
        if self.color != Color::None {
            r = r.and(write!(f, "_{}", self.color))
        }
        if self.time != Time::None {
            r = r.and(write!(f, "_{}", self.time))
        }
        r.and(write!(f, ".pgn"))
    }
}
