class Codegraph < Formula
  desc "Lightning-fast codebase intelligence MCP server"
  homepage "https://github.com/nickolay-grechkin/codegraph"
  version "0.2.0"
  license "MIT"

  if OS.mac?
    if Hardware::CPU.arm?
      url "https://github.com/nickolay-grechkin/codegraph/releases/download/v#{version}/codegraph-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER"
    else
      url "https://github.com/nickolay-grechkin/codegraph/releases/download/v#{version}/codegraph-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER"
    end
  elsif OS.linux?
    url "https://github.com/nickolay-grechkin/codegraph/releases/download/v#{version}/codegraph-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "PLACEHOLDER"
  end

  def install
    bin.install "codegraph"
  end

  test do
    assert_match "codegraph", shell_output("#{bin}/codegraph --version")
  end
end
