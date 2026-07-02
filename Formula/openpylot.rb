# Homebrew formula for OpenPylot.
#
# To install from the tap:
#   brew tap globalmindventures/tap
#   brew install openpylot
#
# Or directly:
#   brew install globalmindventures/tap/openpylot

class Openpylot < Formula
  desc "Rust-powered personal AI assistant with calendar, Telegram, and more"
  homepage "https://github.com/globalmindventures/OpenPylot"
  version "0.3.0"
  license "Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/globalmindventures/OpenPylot/releases/download/v#{version}/pylot-darwin-arm64.tar.gz"
      sha256 "e109e29320379869f6c30cebfd155ed28d9e1ad446869a631f99c3f449bfd717"
    else
      url "https://github.com/globalmindventures/OpenPylot/releases/download/v#{version}/pylot-darwin-x86_64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_X86_64"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/globalmindventures/OpenPylot/releases/download/v#{version}/pylot-linux-arm64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_ARM64"
    else
      url "https://github.com/globalmindventures/OpenPylot/releases/download/v#{version}/pylot-linux-x86_64.tar.gz"
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
        brew services start openpylot

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
    assert_match version.to_s, shell_output("#{bin}/pylot --version")
  end
end
