# Homebrew formula for mdmost. This repository is its own tap.
#
# The release build rewrites the version line, the four sha256 lines by their
# marker comments and the bottle block between the BOTTLE markers. Keep the
# markers, and keep each url line directly above its sha256 line.
class Mdmost < Formula
  desc "Full-screen terminal pager for a single Markdown document"
  homepage "https://github.com/oetiker/mdmost"
  version "0.8.0"
  license "MIT"

  # BOTTLE-START
  bottle do
    root_url "https://github.com/oetiker/mdmost/releases/download/v0.8.0"
    sha256 cellar: :any_skip_relocation, arm64_big_sur: "ab9efbcb57c3bd64a2cfefefd6708d2300ff09e8e716f8514b2f148baa0d0b28"
  end
  # BOTTLE-END

  on_macos do
    on_arm do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "0c1359f7d61ada65e39e1cb22f2e4298aa4a0f551348fa007536ffb69357993d" # mac-arm
    end
    on_intel do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "f56bf8c3c9a5f9bf3c357d80b06e1b987b353e66bf696d2c6dfe9a9a84cb65a4" # mac-x86
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-x86_64-unknown-linux-musl.tar.gz"
      sha256 "98669b5113fa3a83d2dbcb703355a5a5ec3f315032bc52c8e123bf9ab5724761" # linux-x86
    end
    on_arm do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-aarch64-unknown-linux-musl.tar.gz"
      sha256 "b0e3e86e9b17a36182ef703cc37a2d400300f08c095bab9de7953ad59a31ef95" # linux-arm
    end
  end

  def install
    bin.install "mdmost"
    Dir["man/*.[1-9]"].each { |page| (man/"man#{page[-1]}").install page }
    # Terminal configuration fragments, as examples. Nothing loads them from here:
    # WezTerm and kitty each read one file owned by the user, so these are copied by
    # hand. See integrations/README.md, and mdmost(1) DEFAULT MARKDOWN VIEWER.
    pkgshare.install "integrations"
  end

  def caveats
    <<~EOS
      To open Markdown files with mdmost by clicking a file:// link in your
      terminal, copy the fragment for it out of:
        #{opt_pkgshare}/integrations
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/mdmost --version")
    (testpath/"doc.md").write("# Title\n\nBody text.\n")
    assert_match "Body text", shell_output("#{bin}/mdmost --render-once --width 40 #{testpath}/doc.md")
  end
end
