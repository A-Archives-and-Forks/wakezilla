class Wakezilla < Formula
  desc "Wake-on-LAN proxy server written in Rust"
  homepage "https://github.com/guibeira/wakezilla"
  version "0.2.11"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.11/wakezilla-0.2.11-aarch64-apple-darwin.tar.gz"
      sha256 "5f3434630789dcf17e28b267ed79a27ea62e330548a4f5ff97cc325e543e4aaf"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.11/wakezilla-0.2.11-x86_64-apple-darwin.tar.gz"
      sha256 "4ccf2999e33614f230f9d0788376ba1ec373a71e1467f3d965e46c114335332c"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.11/wakezilla-0.2.11-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "10926106cdc629ec53c733796513f7c4fb9abf06a4a961527f79bd49a951711a"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.11/wakezilla-0.2.11-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "555b0f84c0d11a6903c9743f19c27696279fa9ceaecde0c2d5cf4ee949decdb4"
    end
  end

  def install
    bin.install "wakezilla"
  end
end
