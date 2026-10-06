# Homebrew formula for mdmost. This repository is its own tap:
#
#   brew tap oetiker/mdmost https://github.com/oetiker/mdmost
#   brew install mdmost
#
# The version and the four sha256 lines are rewritten by .github/workflows/release-build.yml
# after the release artifacts exist. The trailing marker comments are what that
# rewrite matches on -- do not remove them.
class Mdmost < Formula
  desc "Full-screen terminal pager for a single Markdown document"
  homepage "https://github.com/oetiker/mdmost"
  version "0.7.0"
  license "MIT"

  # Bottles exist for one reason: without one, Homebrew treats this formula as a source
  # build and refuses to install on a Mac whose Command Line Tools are older than its
  # macOS, even though nothing here is compiled. The block is rewritten by
  # .github/workflows/release-build.yml once a release's bottles exist, and the marker
  # comments are the range that rewrite replaces -- do not remove them. Empty means no
  # bottle is published yet, which costs nothing but that check.
  #
  # One bottle per architecture is enough: on macOS, Homebrew falls back to a bottle
  # built for an *older* macOS of the same architecture (find_older_compatible_tag in
  # extend/os/mac/utils/bottles.rb), so these keep working on later releases. That
  # fallback only reaches upward, which is why the bottles are built on the oldest
  # runner image available for each architecture.
  # BOTTLE-START
  bottle do
    root_url "https://github.com/oetiker/mdmost/releases/download/v0.7.0"
    sha256 cellar: :any_skip_relocation, arm64_sonoma: "b36aa74268f6bc19658e84743e11b44acf86a31e2f9f7ff6b379d9f95d3b54a2"
    sha256 cellar: :any_skip_relocation, sequoia: "c3f72a203eaf5a1d948ea87c8fa9fcead79b2e7ddbfb631def07f2c7bf91db4d"
  end
  # BOTTLE-END

  on_macos do
    on_arm do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "3ea7a2d0eb1de1b842ff3a67aaa67dec505378f57c53aa1adc35841ab459e61c" # mac-arm
    end
    on_intel do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "b3d10807636dcf6bb23c5a66e3e2486fe7dd2a8da22fff8c2f7d1ec47198b0c2" # mac-x86
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-x86_64-unknown-linux-musl.tar.gz"
      sha256 "c18faa8a45997f7009d448501e0ef9dab961ca72da0f8f70544957d13df6638d" # linux-x86
    end
    on_arm do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-aarch64-unknown-linux-musl.tar.gz"
      sha256 "f0138089ddada7c16349373ec4d52db60a4ff50ac538e9ff9cc1632688ddaae8" # linux-arm
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
