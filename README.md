# shpell

**Write shell commands in natural language.** Cast a spell at your shell, and
it conjures the command.

```
❯ create an empty file named test
✻ touch test
m3 :: ~/.config ‹main*› » touch test
```

Press **Tab** on an empty line in zsh or bash to enter Shpell mode — a small
interactive UI that lives outside the zsh prompt. Type what you want after
`❯` (arrow keys, paste and ↑-recall all work); the command streams in after
`✻`, which pulses while generating. Then:

- **Enter on an empty line** — accept: back to zsh with the command sitting
  on your prompt, **not executed** — run it, edit it, or throw it away. The
  whole exchange stays on screen above the prompt
- **Keep typing** — refine: ask a follow-up and the command is revised
- **Esc / Ctrl-C / Ctrl-D** — cancel

## Demo

> **TODO(@yangtau):** record a short demo video / GIF and embed it here.

## Highlights

- **Zero interference** — only triggers on Tab at an empty line; Tab anywhere
  else is still your regular completion (plays nice with fzf-tab etc.), and a
  generated command never runs without your confirmation
- **Streaming UI** — commands appear as they are generated, with a Claude
  Code-style pulsing spinner
- **Multi-turn refinement** — not quite right? Just say what to change
- **Uses your ChatGPT or xAI subscription** — OAuth login, no API key, no API credits
- **Clean by construction** — the interaction runs in its own process, never
  through zle, so your natural language is never shell-parsed, highlighted or
  history-expanded

## Install

### Nix

Each push to `main` publishes release binaries. Nix downloads those instead of
compiling:

```sh
nix profile add github:yangtau/shpell
# or in a flake: inputs.shpell.url = "github:yangtau/shpell";
```

A dirty checkout, or a system with no published hash, builds from source.
Force a source build with `nix build .#shpell-from-source`.

### Cargo

```sh
cargo install --path .
```

### Prebuilt binaries

Same tarballs live on the rolling
[`prebuilt`](https://github.com/yangtau/shpell/releases/tag/prebuilt)
release (Linux x86_64/arm64, macOS arm64). Unpack and put `shpell` on
your `PATH`.

## Quick start

1. Log in with a subscription (OAuth, no API key):

   ```sh
   shpell auth login                 # ChatGPT (default)
   shpell auth login xai-grok        # SuperGrok / X Premium
   ```

   This prints a login URL; finish authorization in the browser. ChatGPT
   uses the Codex CLI callback on localhost:1455; xAI opens the Grok OAuth
   flow (same SuperGrok / X Premium subscription as Grok Build).

2. Add to the end of `~/.zshrc` — or, for bash (4.0+), `~/.bashrc`:

   ```sh
   eval "$(shpell init zsh)"    # zsh
   eval "$(shpell init bash)"   # bash
   ```

3. Open a new terminal, hit Tab on an empty line, and write commands in plain
   language.

You can also skip the shell integration entirely: bare `shpell` opens the
same interactive Shpell mode, and `shpell find large files` enters it with
that request already submitted.

## Configuration

`~/.config/shpell/config.toml` (optional):

```toml
# ChatGPT subscription (default)
provider = "openai-chatgpt"
model = "gpt-5.4-mini"
reasoning_effort = "low"       # none | low | medium | high | xhigh (varies by model)

# or SuperGrok / X Premium — then `shpell auth login xai-grok`
# provider = "xai-grok"
# model = "grok-4.6"
# reasoning_effort = "low"
```

Icons used in Shpell mode (plain Unicode, any font works; `export` them
before use):

| Variable | Default | Meaning |
|---|---|---|
| `SHPELL_USER_ICON` | `❯` | icon for your input line |
| `SHPELL_AI_ICON` | `✻` | icon for the generated command (resting state) |

## Design

Curious why it triggers via Tab instead of a prefix word, or how the shell
integration and the interactive process split the work? See
[docs/design.md](docs/design.md) (Chinese).

## License

[MIT](LICENSE)

---

中文文档：[README.zh.md](README.zh.md)
