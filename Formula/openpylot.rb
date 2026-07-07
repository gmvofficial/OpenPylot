# Homebrew formula for OpenPylot.
#
# To install from the tap:
#   brew tap gmvofficial/tap
#   brew install openpylot
#
# Or directly:
#   brew install gmvofficial/tap/openpylot

class Openpylot < Formula
  desc "Rust-powered personal AI assistant with calendar, Telegram, and more"
  homepage "https://github.com/gmvofficial/OpenPylot"
  version "0.1.0"
  license "Apache-2.0"

  # Asset names match the tarballs produced by .github/workflows/release-binaries.yml
  # (Rust target triples). After each release, replace the PLACEHOLDER sha256 sums
  # with the values printed in that workflow's run summary.
  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/gmvofficial/OpenPylot/releases/download/v#{version}/pylot-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_DARWIN_ARM64"
    else
      url "https://github.com/gmvofficial/OpenPylot/releases/download/v#{version}/pylot-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_DARWIN_X86_64"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/gmvofficial/OpenPylot/releases/download/v#{version}/pylot-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_ARM64"
    else
      url "https://github.com/gmvofficial/OpenPylot/releases/download/v#{version}/pylot-x86_64-unknown-linux-gnu.tar.gz"
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
