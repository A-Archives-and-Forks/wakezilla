class Wakezilla < Formula
  desc "Wake-on-LAN proxy server written in Rust"
  homepage "https://github.com/guibeira/wakezilla"
  version "0.1.49"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.1.49/wakezilla-0.1.49-aarch64-apple-darwin.tar.gz"
      sha256 "8756f694f2dc42ea880346881637ec4db7f22f1dcd431ae6d4e87c0badc4fe2a"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.1.49/wakezilla-0.1.49-x86_64-apple-darwin.tar.gz"
      sha256 "003d1f8e2ba0c5e6ec4db6acd98af4fae76bbfbfbea33900bb568bbe9954da1f"
    end
  end

  on_linux do
    url "https://github.com/guibeira/wakezilla/releases/download/v0.1.49/wakezilla-0.1.49-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "82e5070be07afec88720401886b17721f8a8daab1937aad452082b208f0cef14"
  end

  def install
    bin.install "wakezilla"
  end
end
