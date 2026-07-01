class Wakezilla < Formula
  desc "Wake-on-LAN proxy server written in Rust"
  homepage "https://github.com/guibeira/wakezilla"
  version "0.2.5"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.5/wakezilla-0.2.5-aarch64-apple-darwin.tar.gz"
      sha256 "a8396ff1a8c17094bfeea0203fb5ac02320e95b01e61f86f6ab6f9dd1fca143d"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.5/wakezilla-0.2.5-x86_64-apple-darwin.tar.gz"
      sha256 "797f981a16e44851ecb27d489e40b588a76dde8d7fb88bfbf85e76a07d8faae3"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.5/wakezilla-0.2.5-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "89778f534ba7fbadc715c6651e8b1d33fd468c5e6c1059273edf5d4a50ede4ad"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.5/wakezilla-0.2.5-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "78751af73b0aa39aecb561e37abf368d6e7d17bc42c69564fc5b7407392db9fb"
    end
  end

  def install
    bin.install "wakezilla"
  end
end
