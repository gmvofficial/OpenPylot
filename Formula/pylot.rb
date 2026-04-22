# Homebrew formula for OpenPylot.
#
# To install from the tap:
#   brew tap openpylot/tap
#   brew install pylot
#
# Or directly:
#   brew install openpylot/tap/pylot

class Pylot < Formula
  desc "Rust-powered personal AI assistant with calendar, Telegram, and more"
  homepage "https://github.com/openpylot/pylot"
  version "0.2.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/openpylot/pylot/releases/download/v#{version}/pylot-darwin-arm64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_ARM64"
    else
      url "https://github.com/openpylot/pylot/releases/download/v#{version}/pylot-darwin-x86_64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_X86_64"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/openpylot/pylot/releases/download/v#{version}/pylot-linux-arm64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_ARM64"
    else
      url "https://github.com/openpylot/pylot/releases/download/v#{version}/pylot-linux-x86_64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_X86_64"
    end
  end

  def install
    bin.install "pylot"
  end

  def post_install
    (var/"pylot/data").mkpath
    (var/"pylot/logs").mkpath
  end

  def caveats
    <<~EOS
      To get started, run the interactive setup wizard:
        pylot init

      To run as a background service:
        brew services start pylot

      Configuration is stored in:
        ~/.pylot/
    EOS
  end

  service do
    run [opt_bin/"pylot", "serve", "--foreground"]
    keep_alive true
    log_path var/"pylot/logs/agent.log"
    error_log_path var/"pylot/logs/agent.error.log"
    working_dir var/"pylot"
  end

  test do
    assert_match "OpenPylot", shell_output("#{bin}/pylot --version")
  end
end
