# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

*Crypts of the Unholy*: a terminal roguelike deckbuilder in Rust (edition 2024) on ratatui 0.30. There are three classes (Blood Knight, Assassin, Warlock) and ten floors ending in a boss. It is a single binary crate with no workspace. Needs a truecolor terminal of at least 90x30; it is designed for 120x40.

## Commands

```sh
cargo run --release                                   # play
cargo build --release
cargo test --release -- --test-threads=1              # full suite (single thread is required, see below)
cargo test --release tutorial_plays_through -- --test-threads=1   # one test
cargo test --release bot_runs_all_classes -- --test-threads=1 --nocapture  # prints win rate / deaths-by-floor balance stats
SNAP_DIR=/some/dir SNAP_W=120 SNAP_H=40 cargo test --release snapshot_screens -- --ignored --test-threads=1
                                                      # dump plain-text renders of each screen to SNAP_DIR
```

Tests must run single-threaded. `isolate_saves()` points `XDG_DATA_HOME` at a temp dir with `unsafe { set_var }` so tests never touch the real save. The release profile is size-optimized with `lto` and `panic = "abort"`, so release builds are slow to compile.

## Architecture

**Main loop** (`main.rs`): a fixed loop of roughly 30 fps calls `App::update(dt)`, then `ui::render(&app, frame)`, then drains crossterm events into `App::on_event`. All game state lives in `App` (`app.rs`).

**State machine** (`app.rs`): `App.screen: Screen` (Title, ClassSelect, Map, Combat, Reward, Choice, DeckPick, End), with `App.overlay` (Pause/Deck/Help) on top. Raw key and mouse events are normalized into a private `Input` enum, then dispatched by `handle`/`handle_overlay` according to the current screen. `sel`/`osel` hold the selection index for the screen and the overlay.

**Mouse hit-testing goes through rendering**: `ui::render` clears `app.hits`/`app.ohits` (`RefCell<Vec<(Rect, Hit)>>`) and each widget registers its clickable rect with `app.hit(...)` while drawing. `on_event` resolves clicks and hovers against the rects from the last frame. A new clickable element needs a `hit()` call in its render code; there is no separate layout pass. `render` takes `&App`, so render-time feedback uses `Cell`/`RefCell` (e.g. `deck_cols`).

**Rules vs. presentation**:
- `combat.rs`: `Combat` holds the rules only (deck piles, energy, statuses, enemy actions). It knows nothing about timing or UI. Every observable effect is pushed to `Combat.events: Vec<Ev>`.
- `app.rs` wraps it in `CombatView`, which adds the `Phase` (Player → Enemy{next} stepped on timers → Won/Lost), the target and messages. Each frame `App::update` drains `c.events` into `fx.ingest`.
- `fx.rs` turns events into floating numbers, slashes, shakes, toasts and banners, all keyed off the global clock `fx.t`. Animation ages are measured against `fx.t` (`screen_t0`, `turn_t0`).
- To add a visual reaction, emit an `Ev` from `Combat` and handle it in `Fx::ingest`. Don't call UI code from the rules.

**Content as data**: cards are `CardId` + `upgraded`. `cards::def(id, up)` is the single table of cost, kind, rarity and `Vec<Eff>`, and `Combat::play` interprets the `Eff` list generically. Enemies (`enemies.rs`) choose an `Intent` per turn through `pick_intent`. Statuses are the `St` enum stored in a fixed array (`model::Statuses`). Relics, classes and statuses carry their name, color and help text through methods on the enums (`model.rs`). ASCII art is in `art.rs`, where spaces are transparent.

**Runs and saves** (`run.rs`): `RunState` holds the deck, HP, gold, relics, the map (a `Vec<Vec<RoomKind>>` per floor) and a serializable splitmix64 `Rng` (`rng.rs`). The whole `RunState` is saved as JSON to `$XDG_DATA_HOME/crypts_of_the_unholy/save.json` before each room is entered. Combat is never serialized, so resuming replays the room from its entry state, and rooms must stay deterministic from the saved RNG. `save_and_resume_mid_room` tests this.

**Tutorial** (`tutorial.rs`): a scripted `Step`/`Goal` sequence that runs as a normal `Combat` on a throwaway `RunState`. While `app.tutorial` is `Some`, `App::save` does nothing, cards can be refused (`blocks_card`), ending the turn is gated (`allows_end_turn`), the pause menu changes, and `ui.rs` draws a guide panel.

**Rendering** (`ui.rs`, `theme.rs`): one `render_*` function per screen, built on low-level helpers that write straight into the `Buffer` (`put`, `fill`, `draw_art`, `embers`) instead of mostly using widgets. All colors are `Color::Rgb` constants in `theme.rs`. Layouts must still work at 90x30, because the bot tests render at both 120x40 and 90x30.

**Tests** (`tests.rs`): headless `TestBackend`. A heuristic bot plays 90 full runs through the real `on_event` and `render` paths, so any panic, layout overflow or stuck state in the UI fails the suite. Changes to the input flow (new screens, confirmations) may require teaching the bot in `play_run`.
