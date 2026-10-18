# FilmCraft — instructions for agents

FilmCraft is a clean-room, open-source, pure-Rust non-linear video editor targeting Adobe Premiere Pro parity (and beyond). Native on macOS, Windows, Linux; web via WASM. Sibling of `../photocraft` (Photoshop), `../pdfcraft` (Acrobat) and `../vectorcraft` (Illustrator; formerly DrawCraft), with the same conventions.

## Start every session here
`plan/` is maintainer-local (gitignored, not in the repo). Each step names the public equivalent to use if you don't have it.
1. Read `plan/STATUS.md` (current milestone, next unchecked task, blockers). Public: [`ROADMAP.md`](ROADMAP.md), then [`docs/gaps.md`](docs/gaps.md) (ranked shortfalls) and [`docs/roadmap.md`](docs/roadmap.md) (current focus).
2. Read the task in `plan/execution-plan.md` §3, the relevant section of `plan/architecture.md`, and the README/docs of the crate you touch. Public: [`docs/architecture.md`](docs/architecture.md), [`docs/contributing.md`](docs/contributing.md), [`docs/testing.md`](docs/testing.md). Visual/behaviour reference: `plan/premiere/` (local only, never committed).
3. Follow the autonomous operation protocol (`plan/execution-plan.md` §7; public summary: [`docs/agents.md`](docs/agents.md) §5–7). Don't stop to ask unless it lists the decision as the user's.

## Non-negotiables
**Read [`AGENTS.md`](AGENTS.md) first; its rules override everything here.**
- **Never crash** ([`AGENTS.md`](AGENTS.md) §0, standard: [`craftrules/standards/never-crash.md`](https://github.com/storytold/craftrules/blob/main/standards/never-crash.md)). People depend on FilmCraft; a crash loses their work, so this outranks feature work. No `unwrap`/`expect`/`panic!`/`unreachable!`/`todo!`/`unimplemented!` or `unsafe` outside tests: return `Result<T, E>` and propagate with `?` (`ok_or`, `let … else`, `if let`). Treat every number from media, project files, presets or CLI/MCP/control params as hostile: `get()` instead of `[i]`, checked/saturating math, no division by possibly-zero values, capped allocations, bounded recursion. `lock().unwrap_or_else(PoisonError::into_inner)`; background jobs run under `catch_unwind`. The panic hook + UI `catch_unwind` (`crates/ui-egui/src/crash.rs`) are the last-resort guard, not a licence. Every crash fix gets a regression test that panicked before. In particular (§1): never use Adobe iconography, images or any other Adobe asset. Every asset must be openly licensed (OSS / public domain / Creative Commons, or original work by a contributor) and must have a `<file>.attribution` sidecar plus an entry in `ATTRIBUTION.md`. `cargo xtask assets` enforces this.
- **Fonts** live in [storytold/craft-fonts](https://github.com/storytold/craft-fonts), never in this repo (no new font files here; [`AGENTS.md`](AGENTS.md) §1.9, standard: [`craftrules/standards/fonts.md`](https://github.com/storytold/craftrules/blob/main/standards/fonts.md)). Build with them: `git clone https://github.com/storytold/craft-fonts ../craft-fonts && CRAFT_FONTS_DIR="$PWD/../craft-fonts" cargo run -p filmcraft` (`crates/text/build.rs` embeds them as `filmcraft_text::fonts::CRAFT_FONTS`, used for Japanese in the UI and titles). Unset, `CRAFT_FONTS` is empty and everything must still build, test and run; never add craft-fonts to a `Cargo.toml`. Tests on its glyphs skip when it is empty, so run the gates with and without `CRAFT_FONTS_DIR` when you touch fonts.
- **Clean-room.** Never read/disassemble anything inside Adobe app bundles (names/listings only). Never copy Adobe icons, presets, LUTs, fonts. Never copy GPL/LGPL/AGPL code (FFmpeg, x264, x265, MLT, Kdenlive, Shotcut, Olive, LAME…). ffmpeg/ffprobe run only as external test oracles / fixture generators — never linked or shipped.
- **Pure Rust** in the product. OS media APIs only via Rust bindings in `crates/platform` (hardware decoding; the one crate allowed `unsafe`, see `AGENTS.md` §0.3 and `docs/adr/0001-platform-ffi.md`), optional, behind traits, with our decoders as the fallback.
- **Layering** ([`docs/architecture.md`](docs/architecture.md) §1, enforced by `cargo xtask layers`): nothing below L5 depends on egui/eframe/winit/rfd/cpal. L0 codec/container crates depend only on `filmcraft-bitstream`.
- **Exact time:** all time is `filmcraft_time::Tick` (254 016 000 000/s). Never use f64 seconds for edit math.
- **Everything is a command** (`crates/engine`): id, label, menu path, shortcut, params, enabled(), run(). UI, CLI, control channel and MCP all dispatch by id.
- **Everything is agent-drivable:** every interactive widget registers an automation id; UI state is serde so the control channel can read/set it.
- **Quality gates** before every commit: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo xtask layers`, `cargo xtask assets`, `cargo xtask wasm` (checks L0–L4 crates). `cargo xtask ci` runs them all.
- **Commits:** one task id per commit (`M2.4: CABAC residual decoding`). Only green states. End messages with the attribution line required by the environment.

## Running and looking at the app
- `cargo run -p filmcraft -- --control 9876` opens the desktop app with the JSON-lines control server (see `docs/control-protocol.md`).
- For UI work, **look at the result**: drive via the control channel and take `ui.screenshot`, compare with `plan/premiere/screenshots/` (maintainer-local; see [`docs/agents.md`](docs/agents.md) §3).
- Parallel agents: separate git worktrees and `CARGO_TARGET_DIR=target/agent-<name>`; keep every `Cargo.toml` valid at all times (the `crates/*` glob means one broken manifest breaks everyone).
- Test fixtures: generate with ffmpeg into `target/fixtures/` (never commit media). See [`docs/testing.md`](docs/testing.md).

## AYP caption rules (this fork)
Vietnamese captions (`crates/edit/src/vi_captions.rs`, command `transcript.createCaptionsVi`) follow the owner's standard: *every element placed has a meaning* — captions should read like a finished product. Any change must keep these, each guarded by a test in `crates/edit/src/vi_captions/tests.rs`:
- 6–10 words a caption, one complete idea; a one- or two-word sentence joins the caption before it (`captions_hold_six_to_ten_words`, `tiny_sentences_join_the_previous_caption`).
- A caption that fits one line (21 characters at size 48, vertical) is not broken; two lines are balanced; **a line never holds a single word** (`no_lonely_word_and_no_needless_break`).
- Never split a compound (built-in list + Wiktionary, CC BY-SA), a number from its unit, a keyword phrase, or an English phrase like "lay off" (`english_phrases_stay_whole`, `everyday_compounds_are_known`, `keywords_and_number_words_are_never_split`).
- Lines start where a clause starts (nếu/thì/mà/vì/để/nhưng…) (`lines_break_before_a_clause`); captions are cut where their lines read well (`captions_are_cut_where_their_lines_read_well`).
- ASR fixes (`replace`) are applied before any length decision (`replacements_come_before_line_breaks`).
- A sentence-final particle (ơi, à, ạ, nhé, nha, nhỉ, hả, nè…) stays with the words before it — it never opens a caption or a line (`particles_stay_with_what_they_end`).
- Overlapping compounds break between the whole words ("hoàn thành | công việc") (`overlapping_compounds_break_between_them`); a very long pause-free stretch is cut into even blocks, never leaving a one-word caption (`very_long_stretches_leave_no_leftover`).
- When the fewest captions can't lay out cleanly, a phrase gets one more caption (`one_more_caption_beats_a_crowded_one`).
- A common word the ASR capitalised mid-text ("phấn đấu Em mới vô…") starts a sentence even without a full stop: captions break before it (`a_capitalised_word_starts_a_new_caption`).
- The numbers are the user's to tune in `~/.capcut-sub/config.txt` (`min_words`, `max_words`, `max_line_chars`) (`caption_numbers_come_from_config`).
A caption bug found by the owner is fixed as a rule with a test, never only by editing that one caption.
