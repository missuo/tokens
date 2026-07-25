# Packaging — `tokens` CLI + background submit service

The `tokens` binary ships a long-running command, `tokens serve`, that submits
your usage on an interval (default **30 min**, override with `--interval <min>`
or `TOKENS_SUBMIT_INTERVAL`). A service manager keeps it alive and starts it at
login/boot — no cron required.

```
tokens serve [--interval <minutes>] [--client <name>...]
```

It submits immediately on start, then every interval; failed submits are logged
and retried next cycle (it never crashes the loop), and SIGTERM/Ctrl-C stops it
cleanly (there is no durable state to flush).

## macOS / Linuxbrew — Homebrew service

```sh
brew install <your-org>/tap/tokens   # see homebrew/tokens.rb
tokens login                          # one-time auth
brew services start tokens            # keep-alive + start at login/boot
brew services info tokens
brew services stop tokens
```

`homebrew/tokens.rb` declares the service via the `service do … end` block
(`run [opt_bin/"tokens", "serve"]`, `keep_alive true`, `run_at_load true`),
which Homebrew renders to a launchd plist (macOS) or systemd unit (Linuxbrew).

## Linux without Homebrew — systemd user service

See `systemd/tokens.service`:

```sh
mkdir -p ~/.config/systemd/user
cp systemd/tokens.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now tokens
sudo loginctl enable-linger "$USER"   # start at boot without a login session
journalctl --user -u tokens -f        # logs
```
