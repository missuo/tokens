# Packaging — `tokens` CLI + scheduled submit service

The recommended service mode is scheduled submit: the service manager wakes up
every **30 min**, runs `tokens --no-spinner submit`, then exits. There is no
long-lived `tokens` process between runs.

```
tokens --no-spinner submit
```

`tokens serve [--interval <minutes>] [--client <name>...]` remains available for
manual long-running use, but packages should prefer scheduled submit to reduce
idle memory and battery impact.

## macOS / Linuxbrew — Homebrew service

```sh
brew install <your-org>/tap/tokens   # see homebrew/tokens.rb
tokens login                          # one-time auth
brew services start tokens            # run at login/boot, then every 30 min
brew services info tokens
brew services stop tokens
```

`homebrew/tokens.rb` declares the service via the `service do … end` block
(`run [opt_bin/"tokens", "--no-spinner", "submit"]`, `run_type :interval`,
`interval 1800`, `run_at_load true`), which Homebrew renders to a launchd
StartInterval job (macOS) or a systemd timer (Linuxbrew).

## Linux without Homebrew — systemd user service

See `systemd/tokens.service`:

```sh
mkdir -p ~/.config/systemd/user
cp systemd/tokens.service systemd/tokens.timer ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now tokens.timer
sudo loginctl enable-linger "$USER"   # start at boot without a login session
journalctl --user -u tokens -f        # logs
```
