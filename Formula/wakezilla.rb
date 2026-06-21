class Wakezilla < Formula
  desc "Wake-on-LAN proxy server written in Rust"
  homepage "https://github.com/guibeira/wakezilla"
  version "0.2.4"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.4/wakezilla-0.2.4-aarch64-apple-darwin.tar.gz"
      sha256 "4b3749e0008eace0c4f6952c40f353b747b4be1c4dad51989ed93c5ce17c0cc8"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.4/wakezilla-0.2.4-x86_64-apple-darwin.tar.gz"
      sha256 "3a53b5cd4f05e012db8b4ddcef1b6af42cbb4c0b49474dba84bbc5574bc921d4"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.4/wakezilla-0.2.4-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "c4acf826b0ef383606cababbaaac33e8748d1dceeed7908884a7b47846ae6a1f"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.4/wakezilla-0.2.4-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "f2b0e2b2595d2f25723cffd6c1a6d06b9a397f03e1a9fbb383494dd76957f564"
    end
  end

  def install
    bin.install "wakezilla"
  end
end
