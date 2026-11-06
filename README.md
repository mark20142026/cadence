# Cadence

A from-scratch reimplementation of Geometry Dash 2.2091 in Rust, for desktop and the browser.

[![Rust](https://img.shields.io/badge/Rust-1.90%2B-orange?logo=rust)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/github/license/mark20142026/cadence)](LICENSE)
[![Release](https://img.shields.io/github/v/release/mark20142026/cadence)](https://github.com/mark20142026/cadence/releases/latest)
[![Platforms](https://img.shields.io/badge/platforms-macOS%20(Metal)%20%7C%20Windows%20%7C%20Linux%20%7C%20WebAssembly-blue)](https://github.com/mark20142026/cadence/releases/latest)

Cadence ships **none** of the original game's files. You point it at your own copy of the
game, and it reads the textures, fonts, sounds and levels from there. It is not affiliated with
or endorsed by RobTop Games.

**Built in under 72 hours with Claude Opus 5.5** (the first public version after 50). Cadence is
written mostly by Claude Opus 5.5
(Anthropic's AI model), with Mark directing, playing and testing it against the real game; the
day-by-day log is in [JOURNAL.md](JOURNAL.md).

<!--
  SHOWCASE: put a GIF or short video here, Cadence and the real game side by side on the same
  level (same inputs). Keep it under ~10 MB so it loads on the first screen, e.g.
  <p align="center"><img src="docs/showcase.gif" alt="Cadence and Geometry Dash 2.2091 side by side" width="720"></p>
-->

**[Download v0.0.2](https://github.com/mark20142026/cadence/releases/latest)** for macOS (Apple
Silicon or Intel), Windows or Linux. You need your own copy of Geometry Dash 2.2091.

## Why Cadence?

| | |
|---|---|
| **Same physics, step for step** | Position, velocity and game mode match the real game at every one of its 240 physics steps per second, on 135 recorded runs across the 22 main levels, The Challenge and The Tower ([PHYSICS.md](PHYSICS.md)). |
| **Same camera and rotation** | The camera matches bit for bit in all 135 runs, the drawn rotation in 134. |
| **Steadier on heavy levels** | On 15 of the heaviest online levels (MacBook Air M2), Cadence averaged 59.8 FPS or better on every one; the real game fell to 48.3 on Lag Machine. Frames over 20 ms: 21 against 1,351 ([BENCHMARKS.md](BENCHMARKS.md)). |
| **Lighter** | In the same run, about a quarter less CPU (29.0% against 39.5% of a core) and less memory (593 against 701 MB). With the real game focused and nothing else running it used less CPU than Cadence; both runs are in BENCHMARKS.md. |
| **Native and in the browser** | Apple Silicon and Intel Macs (Metal), Windows and Linux, plus a WebAssembly build (WebGPU, falling back to WebGL2) that holds 60 FPS once a level has loaded. |
| **Online like the game** | Search, downloads, comments and accounts send the game's own requests, byte for byte. |
| **Your files stay yours** | Reads your copy of the game and never writes to its folder or its save. |

## Status

Early development (v0.0.2). What works:

- The main levels and The Tower play with the original physics: player positions match the real
  game step for step on the recorded routes (`tools/verify_all.sh`), and so do the camera (all
  135 suites) and the drawn rotation (134). [PHYSICS.md](PHYSICS.md) sets out what is exact,
  what is close and what is ported but not yet checked.
- The menus: level select, settings (all pages), keybindings (every action of the game,
  rebindable, export / import), icon kit (with the trails, ship fires and death effects pages),
  shops (buy with Mana Orbs and Diamond Shards), Paths of Power, Shards of Power, the Treasure
  Room (chests opened with keys), vaults (with their codes), achievements (with their banners),
  stats, news, quests.
- Currencies earned like the game's: orbs, diamonds, keys, shards; chests open with the game's
  animation.
- Online levels: search (all filters), the quick searches, map packs, gauntlets (with their
  chests), lists, the daily/weekly/event levels, level pages, comments, profiles, leaderboards
  (global and per level). Every request is built like the game's own, byte for byte (checked
  against requests recorded from the real game).
- Accounts: log in (GJP2 only, the password is never stored), register, cloud Save / Load
  (merging, values only go up), your profile and posts, friends, requests, following, blocked,
  messages, likes, comments, reports, ratings, level upload. Writes are opt-in.
- Online play in the browser, through a small relay (`relay/`, a Cloudflare Worker that only
  passes requests on).
- The level editor: build/edit/delete, the object tabs, groups, colors, every trigger's popup
  (sliders included), the particle editor, level settings, playtest, music, export/import
  (`.gmd`), saving your own levels.
- 2.0–2.2 triggers, including advanced follow, keyframes, enter effects, random, area triggers.
- Death effects 1–20 as the game plays them.
- Performance: on 15 of the heaviest online levels Cadence holds 60 FPS with identical
  pictures, where the original drops frames on 7 of them, and uses about a quarter less CPU
  and less memory on most (see [BENCHMARKS.md](BENCHMARKS.md)).

## Download and play

Get the package for your computer from [Releases](https://github.com/mark20142026/cadence/releases)
and run **Cadence**. The launcher finds your copy of Geometry Dash 2.2091 in any Steam library
(or a folder you pick), or downloads it with your Steam login through Valve's SteamCMD (you need
to own the game; your password goes to SteamCMD only), then starts Cadence on it.

- macOS: the `.dmg` (Apple Silicon or Intel). It isn't notarized: the first time, right-click
  Cadence and choose **Open**.
- Windows: unzip and run `cadence.exe`.
- Linux: unpack and run `cadence`.

## Building it yourself

You need a legal copy of Geometry Dash 2.2091 and Rust 1.90+.

```bash
cargo run --release -p cadence-launcher   # the launcher
cargo run --release -p cadence-game       # the game directly
```

`tools/package.sh` builds the player package for your platform into `dist/`.

Cadence finds the game's resources in the usual Steam location; see
[DOCUMENTATION.md](DOCUMENTATION.md) for other locations, the web build and all options.

Your progress is kept in Cadence's own save file. Cadence never writes to the game's folder or
its save.

## Development

- `JOURNAL.md`: what changed and why, night by night.
- `NOTES.md`: how the original game works, as found in the binary.
- `tools/`: verification and comparison scripts; `harness/`: a Geode mod used locally to record
  the real game for comparisons.
- Not in the repository (see `.gitignore`): `re/` (the reverse-engineering workspace),
  `third_party/` (Geode's bindings, which have no stated license), `shots/` and
  `bench-levels/` (screenshots of and levels from the real game). The comparison tools expect
  you to make your own.

## License

MIT, see [LICENSE](LICENSE). This covers Cadence's own code only: the original game's files,
art, music and names belong to RobTop Games and are not included.
