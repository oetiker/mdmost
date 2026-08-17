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
  version "0.1.1"
  license "MIT"

  # Bottles exist for one reason: without one, Homebrew treats this formula as a source
  # build and refuses to install on a Mac whose Command Line Tools are older than its
  # macOS, even though nothing here is compiled. The block is rewritten by
  # .github/workflows/release.yml once a release's bottles exist, and the marker
  # comments are the range that rewrite replaces — do not remove them. Empty means no
  # bottle is published yet, which costs nothing but that check.
  #
  # One bottle per architecture is enough: on macOS, Homebrew falls back to a bottle
  # built for an *older* macOS of the same architecture (find_older_compatible_tag in
  # extend/os/mac/utils/bottles.rb), so these keep working on later releases. That
  # fallback only reaches upward, which is why the bottles are built on the oldest
  # runner image available for each architecture.
  # BOTTLE-START
  bottle do
  end
  # BOTTLE-END

  on_macos do
    on_arm do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "2ebbbff1042afa5a3843d6554a052d4c66e61da632bfd3bb456ee0031ea7dec2" # mac-arm
    end
    on_intel do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "f9ea3812f5d5851e357e8793f8108ec3e4df6519e0e15d598d83159a64e67ba5" # mac-x86
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-x86_64-unknown-linux-musl.tar.gz"
      sha256 "938342c0be2c27eb6ea4979a2505543d178a75252c80dea87607469f924e3fda" # linux-x86
    end
    on_arm do
      url "https://github.com/oetiker/mdmost/releases/download/v#{version}/mdmost-#{version}-aarch64-unknown-linux-musl.tar.gz"
      sha256 "38651302c03bf6be6111a8771cba3598c191adfe97758c02848c39253f60cd34" # linux-arm
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
