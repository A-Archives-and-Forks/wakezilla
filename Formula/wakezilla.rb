class Wakezilla < Formula
  desc "Wake-on-LAN proxy server written in Rust"
  homepage "https://github.com/guibeira/wakezilla"
  version "0.2.2"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.2/wakezilla-0.2.2-aarch64-apple-darwin.tar.gz"
      sha256 "9398be1afa9809b58b8f99eacca66ddc8d5336560bedd6789b370d9b66ba85e8"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.2/wakezilla-0.2.2-x86_64-apple-darwin.tar.gz"
      sha256 "9f6aad5472c40a8c4baae0ddaa03dda37784b53842d97541265f56b1855d77da"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.2/wakezilla-0.2.2-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "46bb947c8a7d68b80a85416a5d14b7190d90cb958a20bbe079a9ee98b5c471fc"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.2/wakezilla-0.2.2-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "10c7be4f9bfbb0cd8806df118771c8df0ba8beea363243fb9492f634d0504580"
    end
  end

  def install
    bin.install "wakezilla"
  end
end
