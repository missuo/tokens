# Homebrew formula for the `tokens` CLI + background submit service.
#
# This installs the `tokens` binary and registers a scheduled background service
# that runs `tokens --no-spinner submit` every 30 minutes. On macOS Homebrew
# renders this to a launchd StartInterval job; on Linuxbrew it renders a systemd
# timer. The CLI exits between runs instead of keeping a long-lived process.
#
# Build-from-source variant (simplest to get going on a personal tap — no
# prebuilt bottles or per-platform sha256 to maintain). Swap to a bottle/
# prebuilt-binary formula later if you want faster installs.
#
# Usage once published to a tap (e.g. `<your-org>/homebrew-tap`):
#   brew install <your-org>/tap/tokens
#   tokens login
#   brew services start tokens      # scheduled submit at login/boot
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
    run [opt_bin/"tokens", "--no-spinner", "submit"]
    run_type :interval
    interval 1800          # 30 minutes
    run_at_load true       # submit once at login / boot, then on interval
    log_path var/"log/tokens.log"
    error_log_path var/"log/tokens.log"
  end

  test do
    assert_match "tokens", shell_output("#{bin}/tokens --version")
  end
end
