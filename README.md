# Tokens

> Track, visualize, and compete on AI coding-assistant token usage — across Claude Code, Codex, Cursor, OpenCode, Gemini, and more.

> [!NOTE]
> **This project is a fork and secondary development of [Tokscale](https://github.com/junhoyeo/tokscale) by [Junho Yeo](https://github.com/junhoyeo)** (original site: [tokscale.ai](https://tokscale.ai)).
> All credit for the original design and implementation goes to the upstream author and contributors. Like the original, this project is released under the **MIT License** — see [LICENSE](./LICENSE).

**Tokens** is a CLI + web leaderboard for monitoring your AI coding token usage and cost. Install the CLI, sign in once, and your usage submits automatically — then see your stats, contribution graph, and rank at **[tokens.ci](https://tokens.ci)**.

## Install

**One-off, any platform (no install — Bun or Node 18+):**

```sh
bunx tokens-cli --help            # or: npx tokens-cli --help
bunx tokens-cli submit            # run a one-shot submit
```

**Persistent global install:**

```sh
bun add -g tokens-cli             # or: npm install -g tokens-cli
tokens --help                     # the package exposes a `tokens` command on PATH
```

The `tokens-cli` package is a thin launcher that pulls in the matching native binary for your platform (`tokens-cli-<platform>`), so a single install works everywhere and exposes the `tokens` command.

**macOS (Homebrew):**

```sh
brew install owo-network/brew/tokens
```

**Linux (one-click — installs the binary and a background service):**

```sh
curl -fsSL https://s.ee/tokens | bash
```

## Automatic submission

Sign in once, then let `tokens` submit your usage in the background on an interval (default 30 min — override with `TOKENS_SUBMIT_INTERVAL`).

**macOS:**

```sh
tokens login                 # one-time sign in with GitHub
brew services start tokens   # start the background submitter (and at login)
tokens status                # verify auth, device, and background service state
```

That's it — your usage now submits automatically. You can also submit once, manually, at any time:

```sh
tokens submit
```

**Linux (systemd, set up by `install.sh`):**

```sh
tokens login                         # one-time sign in
systemctl --user start tokens        # start the background submitter
systemctl --user enable tokens       # start automatically on boot …
sudo loginctl enable-linger "$USER"  # … even without an active login session
```

Logs: `journalctl --user -u tokens -f` · Service status: `systemctl --user status tokens` · Local check: `tokens status`

## Usage

| Command | What it does |
| --- | --- |
| `tokens` | Interactive TUI dashboard of your token usage |
| `tokens login` | Sign in with GitHub (one-time) |
| `tokens status` | Check local auth, device, and background service state |
| `tokens submit` | Submit your usage to [tokens.ci](https://tokens.ci) |
| `tokens serve` | Run the background submitter (used by the service above) |
| `tokens graph` | Export your contribution data as JSON |
| `tokens --help` | Full command reference |

## Overview

**Tokens** helps you monitor and analyze your token consumption from:

| Logo | Client | Data Location | Supported |
|------|----------|---------------|-----------|
| <img width="48px" src=".github/assets/client-opencode.png" alt="OpenCode" /> | [OpenCode](https://github.com/sst/opencode) | `~/.local/share/opencode/opencode.db` (1.2+, all channels including `opencode-stable.db`) or/and `~/.local/share/opencode/storage/message/` (legacy/unmigrated) | ✅ Yes |
| <img width="48px" src=".github/assets/client-claude.jpg" alt="Claude" /> | [Claude Code](https://docs.anthropic.com/en/docs/claude-code) | `~/.claude/projects/` and `~/.claude/transcripts/` | ✅ Yes |
| <img width="48px" src=".github/assets/client-openclaw.jpg" alt="OpenClaw" /> | [OpenClaw](https://openclaw.ai/) | `~/.openclaw/agents/` (+ legacy: `.clawdbot`, `.moltbot`, `.moldbot`) | ✅ Yes |
| <img width="48px" src=".github/assets/client-openai.jpg" alt="Codex" /> | [Codex CLI](https://github.com/openai/codex) | `~/.codex/sessions/` | ✅ Yes |
| <img width="48px" src=".github/assets/client-copilot.jpg" alt="Copilot" /> | [GitHub Copilot CLI](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/use-the-github-copilot-coding-agent-in-cli) | `~/.copilot/otel/*.jsonl` (+ `COPILOT_OTEL_FILE_EXPORTER_PATH`) | ✅ Yes |
| <img width="48px" src=".github/assets/client-hermes.png" alt="Hermes Agent" /> | [Hermes Agent](https://github.com/NousResearch/hermes-agent) | `$HERMES_HOME/state.db` (fallback: `~/.hermes/state.db`) | ✅ Yes |
| <img width="48px" src=".github/assets/client-gemini.png" alt="Gemini" /> | [Gemini CLI](https://github.com/google-gemini/gemini-cli) | `$GEMINI_CLI_HOME/tmp/*/chats/*.json` (fallback: `~/.gemini/tmp/*/chats/*.json`) | ✅ Yes |
| <img width="48px" src=".github/assets/client-cursor.jpg" alt="Cursor" /> | [Cursor IDE](https://cursor.com/) | Cursor API export cached at `~/.config/tokens/cursor-cache/usage*.csv` (not `~/.cursor`) | ✅ Yes |
| <img width="48px" src=".github/assets/client-amp.png" alt="Amp" /> | [Amp (AmpCode)](https://ampcode.com/) | `~/.local/share/amp/threads/` | ✅ Yes |
| <img width="48px" src=".github/assets/client-codebuff.png" alt="Codebuff" /> | [Codebuff](https://codebuff.com/) | `~/.config/manicode/` (+ `manicode-dev`, `manicode-staging`; override via `CODEBUFF_DATA_DIR`) | ✅ Yes |
| <img width="48px" src=".github/assets/client-droid.png" alt="Droid" /> | [Droid (Factory Droid)](https://factory.ai/) | `~/.factory/sessions/` | ✅ Yes |
| <img width="48px" src=".github/assets/client-pi.png" alt="Pi" /> | [Pi](https://github.com/badlogic/pi-mono) | `~/.pi/agent/sessions/` and `~/.omp/agent/sessions/` ([Oh My Pi](https://github.com/can1357/oh-my-pi)) | ✅ Yes |
| <img width="48px" src=".github/assets/client-kimi.png" alt="Kimi" /> | [Kimi CLI](https://github.com/MoonshotAI/kimi-cli) | `~/.kimi/sessions/` | ✅ Yes |
| <img width="48px" src=".github/assets/client-qwen.png" alt="Qwen" /> | [Qwen CLI](https://github.com/QwenLM/qwen-cli) | `~/.qwen/projects/` | ✅ Yes |
| <img width="48px" src=".github/assets/client-roocode.png" alt="Roo Code" /> | [Roo Code](https://github.com/RooCodeInc/Roo-Code) | `~/.config/Code/User/globalStorage/rooveterinaryinc.roo-cline/tasks/` (+ server: `~/.vscode-server/data/User/globalStorage/rooveterinaryinc.roo-cline/tasks/`) | ✅ Yes |
| <img width="48px" src=".github/assets/client-kilocode.png" alt="Kilo" /> | [Kilo](https://github.com/Kilo-Org/kilocode) | `~/.config/Code/User/globalStorage/kilocode.kilo-code/tasks/` (+ server: `~/.vscode-server/data/User/globalStorage/kilocode.kilo-code/tasks/`) | ✅ Yes |
| <img width="48px" src=".github/assets/client-kilocode.png" alt="Kilo CLI" /> | [Kilo CLI](https://github.com/nicepkg/kilo) | `~/.local/share/kilo/kilo.db` | ✅ Yes |
| <img width="48px" src=".github/assets/client-mux.png" alt="Mux" /> | [Mux](https://github.com/coder/mux) | `~/.mux/sessions/` | ✅ Yes |
| <img width="48px" src=".github/assets/client-crush.png" alt="Crush" /> | [Crush](https://crush.ai/) | `$XDG_DATA_HOME/crush/projects.json` (project registry; fallback: `~/.local/share/crush/projects.json`) | ✅ Yes |
| <img width="48px" src=".github/assets/client-goose.png" alt="Goose" /> | [Goose](https://github.com/aaif-goose/goose) | `~/.local/share/goose/sessions/sessions.db` (+ macOS Application Support, legacy Block/goose paths; override via `GOOSE_PATH_ROOT`) | ✅ Yes |
| <img width="48px" src=".github/assets/client-antigravity.png" alt="Antigravity" /> | [Google Antigravity](https://antigravity.google/) | Cached via `tokens antigravity sync` to `~/.config/tokens/antigravity-cache/sessions/*.jsonl` (live RPC against the local language server) | ✅ Yes |
| <img width="48px" src=".github/assets/client-trae.png" alt="Trae" /> | [Trae IDE](https://www.trae.ai/) / [Trae Solo](https://www.trae.ai/solo) (international) | Cached via `tokens trae sync` to `~/.config/tokens/trae-cache/sessions/*.json` (account-level usage from the official API) | ✅ Yes |
| Warp/Oz | [Warp](https://www.warp.dev/) / Oz | Cached via `tokens warp sync` to `~/.config/tokens/warp-cache/usage.json` (aggregate requests and spend only; no token transcripts) | ✅ Yes |
| <img width="48px" src=".github/assets/client-zed.webp" alt="Zed Agent" /> | [Zed Agent](https://zed.dev/docs/ai/agent-panel) | `~/.local/share/zed/threads/threads.db` (macOS: `~/Library/Application Support/Zed/threads/threads.db`; Windows: `%LOCALAPPDATA%/Zed/threads/threads.db`; hosted Zed models only, not external ACP agents) | ✅ Yes |
| Kiro | Kiro | `~/.kiro/sessions/cli/*.json` (+ `*.jsonl`) and `~/.local/share/kiro-cli/data.sqlite3` (macOS: `~/Library/Application Support/kiro-cli/data.sqlite3`) | ✅ Yes |
| <img width="48px" src=".github/assets/client-cline.png" alt="Cline" /> | [Cline](https://github.com/cline/cline) | VS Code globalStorage tasks (Linux: `~/.config/Code/...`; macOS: `~/Library/Application Support/Code/...`; Windows: `%APPDATA%\Code\...`; server: `~/.vscode-server/data/User/globalStorage/saoudrizwan.claude-dev/tasks/`) | ✅ Yes |
| <img width="48px" src=".github/assets/client-synthetic.png" alt="Synthetic" /> | [Synthetic](https://synthetic.new/) | Re-attributed from other sources via `hf:` model prefix or `synthetic` provider (+ [Octofriend](https://github.com/synthetic-lab/octofriend): `~/.local/share/octofriend/sqlite.db`) | ✅ Yes |

Get real-time pricing calculations using [🚅 LiteLLM's pricing data](https://github.com/BerriAI/litellm), with support for tiered pricing models and cache token discounts.

All usage is read **locally** from each tool's own data directory — nothing leaves your machine unless you run `tokens submit` or enable the background service.

## Web

- **Leaderboard & profiles:** [tokens.ci](https://tokens.ci)
- **Your profile:** `https://tokens.ci/u/<your-github-username>`
- **Settings → Submission History:** review what you've submitted, per device.

## License

[MIT](./LICENSE) — the same license as the upstream project.

Built on **[Tokscale](https://github.com/junhoyeo/tokscale)** by [Junho Yeo](https://github.com/junhoyeo). Huge thanks to the original author and contributors for the foundational work.
