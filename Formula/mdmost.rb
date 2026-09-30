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
  version "0.5.0"
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
    root_url "https://github.com/oetiker/mdmost/releases/download/v0.5.0"
    sha256 cellar: :any_skip_relocation, arm64_sonoma: "6ccddf91e04f3dfd7fc9c0c81f2b298b594b8cd9e70118aff6bb600463b7c4b9"
    sha256 cellar: :any_skip_relocation, sequoia: "fbf5a7979bf39841746c88301f1a0ee7d8c006439050cc0b09a07f1c9523c71f"
  end
  # BOTTLE-END

  on_macos do
    on_arm do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "37c2563d6d32b702ebabf5ab575d8787d16dbd3b16c0fe2bcceb63b31c2fb2e7" # mac-arm
    end
    on_intel do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "e519c6bb283ac315d8f3d626d274c95109503e62e159b2499ff51caa2ccfe087" # mac-x86
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-x86_64-unknown-linux-musl.tar.gz"
      sha256 "e45af5c1200df73be6081df70f7ead243b684adb29bc9f7808490e2f97399b83" # linux-x86
    end
    on_arm do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-aarch64-unknown-linux-musl.tar.gz"
      sha256 "f3f3407581b69d4b6cc7fcfe3bdaa550dbc3c5b2f3831649b8e4c88238737eeb" # linux-arm
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
