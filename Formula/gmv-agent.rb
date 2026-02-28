# Homebrew formula for GMV Agent.
#
# To install from the tap:
#   brew tap GMV-AI/tap
#   brew install gmv-agent
#
# Or directly:
#   brew install GMV-AI/tap/gmv-agent

class GmvAgent < Formula
  desc "Rust-powered personal AI assistant with calendar, Telegram, and more"
  homepage "https://github.com/GMV-AI/gmv-agent"
  version "0.2.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/GMV-AI/gmv-agent/releases/download/v#{version}/gmv-agent-darwin-arm64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_ARM64"
    else
      url "https://github.com/GMV-AI/gmv-agent/releases/download/v#{version}/gmv-agent-darwin-x86_64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_X86_64"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/GMV-AI/gmv-agent/releases/download/v#{version}/gmv-agent-linux-arm64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_ARM64"
    else
      url "https://github.com/GMV-AI/gmv-agent/releases/download/v#{version}/gmv-agent-linux-x86_64.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_X86_64"
    end
  end

  def install
    bin.install "gmv-agent"
  end

  def post_install
    (var/"gmv-agent/data").mkpath
    (var/"gmv-agent/logs").mkpath
  end

  def caveats
    <<~EOS
      To get started, run the interactive setup wizard:
        gmv-agent init

      To run as a background service:
        brew services start gmv-agent

      Configuration is stored in:
        ~/.gmv-agent/
    EOS
  end

  service do
    run [opt_bin/"gmv-agent", "serve", "--foreground"]
    keep_alive true
    log_path var/"gmv-agent/logs/agent.log"
    error_log_path var/"gmv-agent/logs/agent.error.log"
    working_dir var/"gmv-agent"
  end

  test do
    assert_match "GMV Agent", shell_output("#{bin}/gmv-agent --version")
  end
end
