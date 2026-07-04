class Wakezilla < Formula
  desc "Wake-on-LAN proxy server written in Rust"
  homepage "https://github.com/guibeira/wakezilla"
  version "0.2.9"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.9/wakezilla-0.2.9-aarch64-apple-darwin.tar.gz"
      sha256 "16a6b44a1cf32e21259868b447982ca2e8ae64118801fadd1922ecce5de970b7"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.9/wakezilla-0.2.9-x86_64-apple-darwin.tar.gz"
      sha256 "83d08aa85438b8e49517e854707bc7dafc8a119011f6951b708c82ddf543aa91"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.9/wakezilla-0.2.9-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "2fda572f675cd8362597cc3736abda94e2faf5e58627c74f5e813a38db3e01a4"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.9/wakezilla-0.2.9-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "da177168a508ca0e95433e4782a5420a0baf5858e4afa59773b3c7c226cc927f"
    end
  end

  def install
    bin.install "wakezilla"
  end
end
