# Homebrew formula for the `tokens` CLI + background submit service.
#
# This installs the `tokens` binary and registers a managed background service
# (`tokens serve`) that auto-submits your usage on an interval. On macOS the
# service runs via launchd; on Linuxbrew via systemd. In both cases Homebrew
# keeps it alive and starts it at login/boot.
#
# Build-from-source variant (simplest to get going on a personal tap — no
# prebuilt bottles or per-platform sha256 to maintain). Swap to a bottle/
# prebuilt-binary formula later if you want faster installs.
#
# Usage once published to a tap (e.g. `<your-org>/homebrew-tap`):
#   brew install <your-org>/tap/tokens
#   tokens login
#   brew services start tokens      # keep-alive + start at login/boot
#   brew services info tokens
#   brew services stop tokens
class Tokens < Formula
  desc "AI token usage analytics CLI for tokens.ci (with background submit service)"
  homepage "https://tokens.ci"
  # TODO: point at YOUR fork's source tarball + tag, then `brew install`.
  url "https://github.com/<your-org>/tokens/archive/refs/tags/v3.0.0.tar.gz"
  sha256 "REPLACE_WITH_TARBALL_SHA256"
  license "MIT"
  head "https://github.com/<your-org>/tokens.git", branch: "main"

  depends_on "rust" => :build

  def install
    # The workspace [[bin]] is named "tokens" (crate: tokscale-cli).
    system "cargo", "install", "--locked", "--root", prefix, "--path", "crates/tokscale-cli"
  end

  service do
    run [opt_bin/"tokens", "serve"]
    keep_alive true        # restart if it ever exits
    run_at_load true       # start at login / boot
    log_path var/"log/tokens.log"
    error_log_path var/"log/tokens.log"
    # Override the 30-minute default cadence if you like:
    # environment_variables TOKENS_SUBMIT_INTERVAL: "30"
  end

  test do
    assert_match "tokens", shell_output("#{bin}/tokens --version")
  end
end
