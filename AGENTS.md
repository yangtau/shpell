# AGENTS.md

Guidance for AI coding agents working in this repository.

## Project

`shpell` is a Rust CLI that turns natural language into shell commands. The
binary is invoked two ways: `shpell [request]` (opens interactive Shpell
mode, i.e. `compose`; the optional free text is submitted as the first
request) and via shell integrations (zsh, bash) that run `shpell compose`
when Tab is pressed on an empty prompt line. `shpell gen` is the
non-interactive one-shot entry point, kept for scripting.

## Commands

```sh
cargo build                    # debug build
cargo test                     # unit tests (currently only provider::postprocess)
cargo test postprocess         # run a single test by name filter
cargo build --release
zsh -n src/shell/shpell.zsh    # syntax-check the zsh integration script
bash -n src/shell/shpell.bash  # syntax-check the bash integration script
nix build                      # build via flake (CI parity not required locally)
```

There is no separate lint config; `cargo clippy` and `rustfmt` defaults apply.

Manual smoke test: `./target/debug/shpell auth status`, then
`./target/debug/shpell gen -- "list files"`. `shpell compose` needs a real tty.

## Architecture

The core design is a **two-process split with a strict stdout contract**:

- `src/shell/shpell.zsh` (embedded into the binary via `include_str!`, emitted
  by `shpell init zsh`) hijacks Tab on an empty zsh line. It suspends zle,
  restores a cooked tty, and runs `shpell compose` with stdin/stderr attached
  to the tty while **capturing stdout**. Exit code `0` → stdout (the accepted
  command) is placed on the zsh prompt, NOT executed; anything else → cancel.
  `src/shell/shpell.bash` implements the same contract for bash (≥ 4) via a
  Tab-bound readline macro and a `bind -x` dispatch handler.
- `src/compose.rs` runs the interactive loop (rustyline for input, streaming
  output with a pulsing-spinner painter thread). **All UI goes to stderr;
  stdout carries only the final accepted command.** Breaking this contract
  breaks the zsh integration. Natural-language text deliberately never enters
  zle, so it is never shell-parsed, highlighted, or history-expanded — the
  `docs/design.md` lists previously attempted and abandoned approaches;
  read it before proposing a different trigger mechanism.
- Multi-turn refinement is stateless on the provider side: each follow-up is
  sent as "Previously generated command: `...`. Revise it per this request: ...".

Supporting modules:

- `src/main.rs` — clap CLI. Anything that does not start with a known
  subcommand gets `compose` inserted in front of it (see `SUBCOMMANDS`), so
  bare `shpell` and `shpell <free text>` both open Shpell mode. `--shell`
  defaults derive from `$SHELL`.
- `src/provider/` — `Provider` trait (`generate` with an `on_progress`
  streaming callback) plus `from_config` registry. The only implementation,
  `openai_chatgpt.rs`, calls `chatgpt.com/backend-api/codex/responses`
  (SSE-only Responses API) using a ChatGPT subscription — no API key.
  `postprocess` strips code fences / `$ ` prefixes and keeps the first line.
  New providers: implement the trait, register in `from_config`.
- `src/auth.rs` — OAuth PKCE against `auth.openai.com` using the public Codex
  CLI client id, callback on localhost:1455. Tokens live in the platform data
  dir under `shpell/auth.json` (0600) and auto-refresh.
- `src/config.rs` — `~/.config/shpell/config.toml`. Deliberately XDG-style on
  every platform including macOS (do not switch to `dirs::config_dir()`).
- `src/shell/mod.rs` — maps shell name → embedded integration script. New
  shells: add a script in `src/shell/` and register it in `init_script`.

## Conventions

- Commit messages: single-line conventional commits
  (`feat(compose): ...`, `refactor: ...`), present tense, lowercase.
- User-facing strings and error prefixes use the binary name `shpell`.
- User-facing docs come in two languages — `README.md` (English) and
  `README.zh.md` (Chinese) — keep them in sync. Design rationale lives in
  `docs/design.md` (Chinese); update it when behavior changes (exit codes,
  keybindings, interaction flow).
- CI (`.github/workflows/ci.yml`) builds release artifacts for
  linux-musl/macOS on every push to main; a `v*` tag also publishes a GitHub
  Release with the tarballs.
