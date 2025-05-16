use pest::Parser;
use pest::iterators::Pairs;
use std::error::Error;
use std::fmt;

use crate::types::{Game, Time};

#[derive(pest_derive::Parser)]
#[grammar = "pgn.pest"]
pub struct PGNParser;

#[derive(Debug)]
struct PGNParseError(String);
impl fmt::Display for PGNParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PGN parsing error: {}", self.0)
    }
}
impl Error for PGNParseError {}

pub struct ChessParser<'a> {
    pgn_pairs: Pairs<'a, Rule>,
}

impl<'a> ChessParser<'a> {
    pub fn parse(input: &'a str) -> Result<ChessParser<'a>, Box<dyn Error + Send + Sync + 'a>> {
        let pgn = PGNParser::parse(Rule::games, input)?
            .next()
            .ok_or_else(|| PGNParseError("No 'games' rule found in input".to_string()))?;
        Ok(ChessParser {
            pgn_pairs: pgn.into_inner(),
        })
    }
}
impl<'a> std::iter::Iterator for ChessParser<'a> {
    type Item = Result<Game, Box<dyn Error + Send + Sync + 'a>>;
    fn next(&mut self) -> Option<Self::Item> {
        let game_pair = self.pgn_pairs.next()?;
        match game_pair.as_rule() {
            Rule::game => {
                let mut g = Game {
                    pgn: game_pair.as_str().to_owned(),
                    ..Default::default()
                };
                let result: Result<(), Box<dyn Error + Send + Sync + 'a>> = (|| {
                    for header_line in game_pair.into_inner() {
                        let header_line_str = header_line.as_str().to_owned(); // Capture the string here
                        let mut header_line_in = header_line.into_inner();
                        // header_line
                        let attr_pair = header_line_in.next().ok_or_else(|| {
                            PGNParseError(format!(
                                "Missing attribute in header line: {:?}",
                                header_line_str // Use the captured string
                            ))
                        })?;
                        let val_pair = header_line_in.next().ok_or_else(|| {
                            PGNParseError(format!(
                                "Missing value in header line: {:?}",
                                header_line_str // Use the captured string
                            ))
                        })?;
                        let attr = attr_pair.as_str();
                        let val = val_pair.as_str();

                        match attr {
                            "White" => g.white = val.to_lowercase(),
                            "Black" => g.black = val.to_lowercase(),
                            "TimeControl" => g.time = Time::parse(val),
                            _ => (),
                        }
                    }
                    Ok(())
                })();
                match result {
                    Ok(_) => Some(Ok(g)),
                    Err(e) => Some(Err(e)),
                }
            }
            Rule::EOI => None,
            _ => unreachable!(),
        }
    }
}
