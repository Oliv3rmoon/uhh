#!/bin/sh
# Prints the Homebrew formula for an uhh release, using the checksums the
# release workflow published. Usage: packaging/brew-formula.sh 0.1.0
set -e
version="$1"
[ -n "$version" ] || { echo "usage: $0 <version>" >&2; exit 2; }
base="https://github.com/Oliv3rmoon/uhh/releases/download/v$version"
sum() { curl -fsSL "$base/uhh-v$version-$1.tar.gz.sha256" | cut -d' ' -f1; }
cat <<RUBY
class Uhh < Formula
  desc "Explains a shell command flag by flag, from your own man pages"
  homepage "https://github.com/Oliv3rmoon/uhh"
  version "$version"
  license "MIT"

  on_macos do
    on_arm do
      url "$base/uhh-v$version-aarch64-apple-darwin.tar.gz"
      sha256 "$(sum aarch64-apple-darwin)"
    end
    on_intel do
      url "$base/uhh-v$version-x86_64-apple-darwin.tar.gz"
      sha256 "$(sum x86_64-apple-darwin)"
    end
  end

  on_linux do
    on_arm do
      url "$base/uhh-v$version-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "$(sum aarch64-unknown-linux-gnu)"
    end
    on_intel do
      url "$base/uhh-v$version-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "$(sum x86_64-unknown-linux-gnu)"
    end
  end

  def install
    bin.install "uhh"
  end

  def caveats
    <<~EOS
      To make plain \`uhh\` explain your last command (zsh shown; also bash, fish):
        eval "\$(uhh --init zsh)"
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/uhh --version")
    assert_match "sends what", shell_output("NO_COLOR=1 #{bin}/uhh 'echo hi | cat'")
  end
end
RUBY
