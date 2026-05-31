class Wakezilla < Formula
  desc "Wake-on-LAN proxy server written in Rust"
  homepage "https://github.com/guibeira/wakezilla"
  version "0.2.1"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.1/wakezilla-0.2.1-aarch64-apple-darwin.tar.gz"
      sha256 "accc182e926cf3fa2a4513dc01c3f269c7f88102e0fb111109c406f6b2d5ddf9"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.1/wakezilla-0.2.1-x86_64-apple-darwin.tar.gz"
      sha256 "34bbfd224adac15e63ef9fecac057f50f05455cde202ed4c8d5b578686790cdc"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.1/wakezilla-0.2.1-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "fa65909f2a10b35919ae89ddcb8e7931d1e7700b9ba5e622334e03eee8dd40c9"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.1/wakezilla-0.2.1-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "2b3ef3efe2ff5aeb647e79b349f433361d160e7eb867f517c1e88841823ac3ba"
    end
  end

  def install
    bin.install "wakezilla"
  end
end
