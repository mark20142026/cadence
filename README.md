# Cadence

A from-scratch reimplementation of Geometry Dash 2.2091 in Rust, for desktop and the browser.

Cadence ships **none** of the original game's files. You point it at your own copy of the
game, and it reads the textures, fonts, sounds and levels from there. It is not affiliated with
or endorsed by RobTop Games.

## Status

Early development (v0.0.x). What works:

- The main levels and The Tower play with the original physics: player positions match the real
  game step for step on the recorded routes (`tools/verify_all.sh`).
- The menus: level select, settings (all pages), icon kit, shops, vaults (with their codes),
  achievements, stats, news.
- Online levels: search (with the demon difficulty filter), the quick searches, map packs,
  gauntlets, lists, the daily/weekly/event levels, level pages, comments, profiles,
  leaderboards. Every request is built like the game's own, byte for byte (checked against
  requests recorded from the real game).
- Accounts (in progress): logging in works (Settings → Account → Log in; GJP2 only, the password
  is never stored or sent). Likes, comments, uploads and reward claims are built and tested
  against the real game's requests but not yet in the menus; cloud Save/Load stay off until
  Cadence's progress can be written in the game's save format. Writes will be opt-in.
- Performance: on 15 of the heaviest online levels Cadence holds 60 FPS with identical
  pictures, where the original drops frames on 7 of them, and uses about a quarter less CPU
  and less memory on most (see [BENCHMARKS.md](BENCHMARKS.md)).
- The level editor: build/edit/delete, the object tabs, groups, colors, trigger settings for
  60+ triggers, level settings, playtest, saving your own levels (in Cadence's own save).
- Most 2.0–2.2 triggers (move, rotate, scale, follow, spawn, toggle, count, pickup, item
  compare/edit, timers, collision, area effects, camera, shaders, song/SFX, UI, …).

Not yet: advanced follow, keyframes, several editor tools, the account screens, online play on the web (needs a small relay server).

## Running it

You need a legal copy of Geometry Dash 2.2091 and Rust 1.90+.

```bash
cargo run --release -p cadence-game
```

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
