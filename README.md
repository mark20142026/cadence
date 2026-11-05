# Cadence

A from-scratch reimplementation of Geometry Dash 2.2091 in Rust, for desktop and the browser.

Cadence ships **none** of the original game's files. You point it at your own copy of the
game, and it reads the textures, fonts, sounds and levels from there. It is not affiliated with
or endorsed by RobTop Games.

## Status

Early development (v0.0.2). What works:

- The main levels and The Tower play with the original physics: player positions match the real
  game step for step on the recorded routes (`tools/verify_all.sh`).
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
