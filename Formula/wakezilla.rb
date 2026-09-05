class Wakezilla < Formula
  desc "Wake-on-LAN proxy server written in Rust"
  homepage "https://github.com/guibeira/wakezilla"
  version "0.2.13"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.13/wakezilla-0.2.13-aarch64-apple-darwin.tar.gz"
      sha256 "bd02a78ff945b0b46908bc81845cdb5b8694ce3ca064db89909211461850ddc3"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.13/wakezilla-0.2.13-x86_64-apple-darwin.tar.gz"
      sha256 "5f38e4e0b1abbf5589a94201f0cace4861bbb667ed603ca8bd6a92318d35db19"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.13/wakezilla-0.2.13-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "814297da1530ee08c53c5c5b0b0218c85dce43a5390606cd1a83e07b1cf80765"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.13/wakezilla-0.2.13-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "e3e32ac9cb9c25f8834ffb8eb594ea64128f53fc0934ef6689077cce7bd3cf44"
    end
  end

  def install
    bin.install "wakezilla"
  end
end
