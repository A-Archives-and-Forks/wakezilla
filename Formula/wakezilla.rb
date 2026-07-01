class Wakezilla < Formula
  desc "Wake-on-LAN proxy server written in Rust"
  homepage "https://github.com/guibeira/wakezilla"
  version "0.2.6"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.6/wakezilla-0.2.6-aarch64-apple-darwin.tar.gz"
      sha256 "b93a89407b7956923f07ed0c0f21101fb7efb3b1991b60080092af0b5f7756a2"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.6/wakezilla-0.2.6-x86_64-apple-darwin.tar.gz"
      sha256 "59d523ccb6bf2628023b0293342c50c7099d1df07218f619db5967cbe28a540f"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.6/wakezilla-0.2.6-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "a615bfb0877aaa4dd10a990e6aea6bc1b1714a15c3c759a259fc26db8d1553cf"
    else
      url "https://github.com/guibeira/wakezilla/releases/download/v0.2.6/wakezilla-0.2.6-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "4de72150352179849b8110d2d0e01337822c37ef4212cd3dbc4c823ab89f7d9a"
    end
  end

  def install
    bin.install "wakezilla"
  end
end
