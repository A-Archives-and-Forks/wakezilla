class Wakezilla < Formula
  desc "Wake-on-LAN proxy server written in Rust"
  homepage "https://github.com/guibeira/wakezilla"
  version "0.2.10"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.10/wakezilla-0.2.10-aarch64-apple-darwin.tar.gz"
      sha256 "4c8925ce3bbe107470144714fcbc3ee2f399234c49aaad083402d462fbe1e5bb"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.10/wakezilla-0.2.10-x86_64-apple-darwin.tar.gz"
      sha256 "dc6d64fa1c55e1715af7226b248c462833f1632112261f7d9e5476e223b3e80e"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.10/wakezilla-0.2.10-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "14c92a839d85a19686af75917538871421d76fd8256a6fb28b3cfd16531df6aa"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.10/wakezilla-0.2.10-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "33b1a11885fe1e67dfb8f6a513ab4806f3a6a53d539d0c7bec087716aafa6dd2"
    end
  end

  def install
    bin.install "wakezilla"
  end
end
