NOTE: Not working until I get a chess.com API token.

# chess_dl
Fast Chess.com bulk game downloader. Parses the games in order to sort into colors and time controls.

I made this project to learn async Rust and get an edge in chess tournaments where there is not much time to prepare between rounds. Don't expect beautiful code, but it is fairly fast (much faster than any competing program that I've seen). If anyone has tips on how to organize the code better, it would be appreciated.

## Installation
```
cargo install chess_dl
```

## Example

```
chess_dl hikaru gmwso lyonbeast --blitz --bullet -t
```
