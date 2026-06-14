class Wakezilla < Formula
  desc "Wake-on-LAN proxy server written in Rust"
  homepage "https://github.com/guibeira/wakezilla"
  version "0.2.3"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.3/wakezilla-0.2.3-aarch64-apple-darwin.tar.gz"
      sha256 "7bf3c75b6eac5b9b9c5be0395f46c6ec0c6ba3a7171061144f3778d57aef8808"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.3/wakezilla-0.2.3-x86_64-apple-darwin.tar.gz"
      sha256 "b0259c030ca04cdb9d72e4851c2881b0c02ca3c3c7eb56f5230135c8c9a1e643"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.3/wakezilla-0.2.3-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "779437e94894699fbcd5873bae9e20ce12d36e2b57f5032cec97160fe27109af"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.3/wakezilla-0.2.3-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "42eb21dac65b575a8d10f12716b4fe80686ef7c23daea29eef3ddbfc16397ee7"
    end
  end

  def install
    bin.install "wakezilla"
  end
end
