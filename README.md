# Tokens

> Track, visualize, and compete on AI coding-assistant token usage — across Claude Code, Codex, Cursor, OpenCode, Gemini, and more.

> [!NOTE]
> **This project is a fork and secondary development of [tokscale](https://github.com/junhoyeo/tokscale) by [Junho Yeo](https://github.com/junhoyeo)** (original site: [tokscale.ai](https://tokscale.ai)).
> All credit for the original design and implementation goes to the upstream author and contributors. Like the original, this project is released under the **MIT License** — see [LICENSE](./LICENSE).

**Tokens** is a CLI + web leaderboard for monitoring your AI coding token usage and cost. Install the CLI, sign in once, and your usage submits automatically — then see your stats, contribution graph, and rank at **[tokens.ci](https://tokens.ci)**.

## Install

**macOS (Homebrew):**

```sh
brew install owo-network/brew/tokens
```

**Linux (one-click — installs the binary and a background service):**

```sh
curl -fsSL https://raw.githubusercontent.com/missuo/tokens/main/install.sh | bash
```

## Automatic submission

Sign in once, then let `tokens` submit your usage in the background on an interval (default 30 min — override with `TOKENS_SUBMIT_INTERVAL`).

**macOS:**

```sh
tokens login                 # one-time sign in with GitHub
brew services start tokens   # start the background submitter (and at login)
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

Logs: `journalctl --user -u tokens -f` · Status: `systemctl --user status tokens`

## Usage

| Command | What it does |
| --- | --- |
| `tokens` | Interactive TUI dashboard of your token usage |
| `tokens login` | Sign in with GitHub (one-time) |
| `tokens submit` | Submit your usage to [tokens.ci](https://tokens.ci) |
| `tokens serve` | Run the background submitter (used by the service above) |
| `tokens graph` | Export your contribution data as JSON |
| `tokens --help` | Full command reference |

## Supported clients

Claude Code, Codex CLI, Cursor, OpenCode, Gemini CLI, GitHub Copilot CLI, Droid, Hermes, Pi, Zed, and more. Usage is read **locally** from each tool's own data directory — nothing leaves your machine unless you run `tokens submit` or enable the background service.

## Web

- **Leaderboard & profiles:** [tokens.ci](https://tokens.ci)
- **Your profile:** `https://tokens.ci/u/<your-github-username>`
- **Settings → Submission History:** review what you've submitted, per device.

## License

[MIT](./LICENSE) — the same license as the upstream project.

Built on **[tokscale](https://github.com/junhoyeo/tokscale)** by [Junho Yeo](https://github.com/junhoyeo). Huge thanks to the original author and contributors for the foundational work.
