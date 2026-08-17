# Homebrew formula for mdmost. This repository is its own tap:
#
#   brew tap oetiker/mdmost https://github.com/oetiker/mdmost
#   brew install mdmost
#
# The version and the four sha256 lines are rewritten by .github/workflows/release.yml
# after the release artifacts exist. The trailing marker comments are what that
# rewrite matches on — do not remove them.
class Mdmost < Formula
  desc "Full-screen terminal pager for a single Markdown document"
  homepage "https://github.com/oetiker/mdmost"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "888a6590d20fa4d18d6fe0c682a6df0f4d69064077aed1bb87795ef4c2dda923" # mac-arm
    end
    on_intel do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "fb228a2c73c5854c45137e2c41167f875db57d92d685221bc3687c75f8d2efce" # mac-x86
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-x86_64-unknown-linux-musl.tar.gz"
      sha256 "6cfb66982645fa7f39b747efd5fc3910e1ae8adeafd03b92b8614e705f19f4b1" # linux-x86
    end
    on_arm do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-aarch64-unknown-linux-musl.tar.gz"
      sha256 "bbdf714a19f03648aaee48a0b6844d823f7d936c4ff65aa22391784f585fc9af" # linux-arm
    end
  end

  def install
    bin.install "mdmost"
    man1.install "man/mdmost.1"
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
