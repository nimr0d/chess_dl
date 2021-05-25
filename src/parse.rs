use pest::iterators::Pairs;
use pest::Parser;

use crate::types::{Game, Time};

#[derive(pest_derive::Parser)]
#[grammar = "pgn.pest"]
pub struct PGNParser;

pub struct ChessParser<'a> {
    pgn: Pairs<'a, Rule>,
}

impl<'a> ChessParser<'a> {
    pub fn parse(input: &str) -> ChessParser {
        let pgn = PGNParser::parse(Rule::games, input)
            .expect("failed parse")
            .next()
            .unwrap();
        ChessParser {
            pgn: pgn.into_inner(),
        }
    }
}
impl<'a> std::iter::Iterator for ChessParser<'a> {
    type Item = Game;
    fn next(&mut self) -> Option<Self::Item> {
        let game = self.pgn.next()?;
        match game.as_rule() {
            Rule::game => {
                let mut g = Game::default();
                g.pgn = game.as_str().to_owned();
                for header_line in game.into_inner() {
                    let mut header_line_in = header_line.into_inner();
                    // header_line
                    let attr = header_line_in.next().unwrap().as_str();
                    let val = header_line_in.next().unwrap().as_str();
                    match attr {
                        "White" => g.white = val.to_lowercase(),
                        "Black" => g.black = val.to_lowercase(),
                        "TimeControl" => g.time = Time::parse(val),
                        _ => (),
                    }
                }
                Some(g)
            }
            Rule::EOI => None,
            _ => unreachable!(),
        }
    }
}
