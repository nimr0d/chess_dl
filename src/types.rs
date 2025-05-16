use strum::Display;

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, Display, Default)]
pub enum Time {
    #[default]
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
        // Check for Daily game format '1/seconds'
        let daily_parts: Vec<&str> = val.split('/').collect();
        if daily_parts.len() >= 2 && daily_parts[0] == "1" {
            if let Ok(_) = daily_parts[1].parse::<i32>() {
                return Time::Daily;
            }
        }
        // Existing logic for time + increment
        let parts: Vec<&str> = val.split('+').collect();

        let base_time_str = parts.get(0).unwrap_or(&"");
        let increment_str = parts.get(1).unwrap_or(&"0");

        let base_time = match base_time_str.parse::<i32>() {
            Ok(s) => s,
            Err(_) => return Time::Misc,
        };

        let increment = match increment_str.parse::<i32>() {
            Ok(s) => s,
            Err(_) => return Time::Misc,
        };

        let effective_time = base_time + 40 * increment;

        if effective_time <= 120 {
            Self::Bullet
        } else if effective_time <= 600 {
            Self::Blitz
        } else if base_time <= 7200 {
            // Using the cutoff from the file read
            Self::Rapid
        } else {
            Self::Misc
        }
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
    pub username: Option<String>,
    pub color: Color,
    pub time: Time,
}

impl PGNMetadata {
    pub fn from_game(
        username: &str,
        game: &Game,
        group_time: bool,
        group_users: bool,
        group_color: bool,
    ) -> PGNMetadata {
        PGNMetadata {
            username: if group_users {
                None
            } else {
                Some(username.to_owned())
            },
            color: if group_color {
                Color::None
            } else {
                if username.eq_ignore_ascii_case(&game.white) {
                    Color::White
                } else if username.eq_ignore_ascii_case(&game.black) {
                    Color::Black
                } else {
                    Color::None
                }
            },
            time: if group_time { Time::None } else { game.time },
        }
    }
    pub fn from_username(username: &str, group_users: bool) -> PGNMetadata {
        PGNMetadata {
            username: if group_users {
                None
            } else {
                Some(username.to_owned())
            },
            color: Color::None,
            time: Time::None,
        }
    }
}

impl std::fmt::Display for PGNMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut r = if let Some(u) = &self.username {
            write!(f, "{}", u)
        } else {
            write!(f, "AllUsers")
        };
        if self.color != Color::None {
            r = r.and(write!(f, "_{}", self.color))
        }
        if self.time != Time::None {
            r = r.and(write!(f, "_{}", self.time))
        }
        r.and(write!(f, ".pgn"))
    }
}
