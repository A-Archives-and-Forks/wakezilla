class Wakezilla < Formula
  desc "Wake-on-LAN proxy server written in Rust"
  homepage "https://github.com/guibeira/wakezilla"
  version "0.2.12"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.12/wakezilla-0.2.12-aarch64-apple-darwin.tar.gz"
      sha256 "e9df127d5d0218bfa25ad4cbddff632aac7a18fa1a9d6201739b24bf09c19605"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.12/wakezilla-0.2.12-x86_64-apple-darwin.tar.gz"
      sha256 "b29a2cd30cc3ea6139bc27223d61a3bd10030c2991664c2be4271b5f65607545"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.12/wakezilla-0.2.12-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "fb4f2cd781146e9154025a18a084749550502aa5cd6dca6159bf84da60e79f5e"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.12/wakezilla-0.2.12-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "e13dc25c510696917aa9e23f5fbc2e47e22a3bbbe0b8908ba2d5e694e7ce80c7"
    end
  end

  def install
    bin.install "wakezilla"
  end
end
