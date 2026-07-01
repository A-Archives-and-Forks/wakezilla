class Wakezilla < Formula
  desc "Wake-on-LAN proxy server written in Rust"
  homepage "https://github.com/guibeira/wakezilla"
  version "0.2.7"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.7/wakezilla-0.2.7-aarch64-apple-darwin.tar.gz"
      sha256 "1b7c42e0b3a01eac4d5ec5d5e1391d2fbc421f556b8e107782f4c1c9e08c8b6a"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.7/wakezilla-0.2.7-x86_64-apple-darwin.tar.gz"
      sha256 "103d80efe447d20eed7a2a2128856d775e2657e103c6a80eadf90ec6fc448e72"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.7/wakezilla-0.2.7-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "d9c1d0e575935e869af8651cb552f65c59f040f43d4be13dbd0817da47955c8d"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.7/wakezilla-0.2.7-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "38654d701d0fcefbcc3227d826b3f8b44bf5a344dcbf9b5dd0bc43f0801a33a0"
    end
  end

  def install
    bin.install "wakezilla"
  end
end
