class Wakezilla < Formula
  desc "Wake-on-LAN proxy server written in Rust"
  homepage "https://github.com/guibeira/wakezilla"
  version "0.2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.0/wakezilla-0.2.0-aarch64-apple-darwin.tar.gz"
      sha256 "f2de09698b75985651af978b171abde48fd5b41382de0ff6f5452f2a5ab32879"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.0/wakezilla-0.2.0-x86_64-apple-darwin.tar.gz"
      sha256 "99164e236af1db93e19537a1745c170a2afd91f73d892d5205b64e239bb45deb"
    end
  end

  on_linux do
    url "https://github.com/guibeira/wakezilla/releases/download/v0.2.0/wakezilla-0.2.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "ddc7bb361433c838d1b4c93e900c085ef331f91f53b0de2a42a6fcf05bd3b2e6"
  end

  def install
    bin.install "wakezilla"
  end
end
