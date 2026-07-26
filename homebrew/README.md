# homebrew-vilan

The Homebrew tap for [Vilan](https://github.com/vilan-lang/vilan) — a
language that compiles to JavaScript, with a compiler, a formatter, and a
language server in one toolchain.

```sh
brew install vilan-lang/vilan/vilan
```

or, if you would rather tap first:

```sh
brew tap vilan-lang/vilan
brew install vilan
```

Either installs two binaries — `vilan` (the compiler and CLI) and
`vilan-lsp` (the language server the editor extensions speak to) — on macOS
and Linux, Apple silicon and x86-64 alike. You will also need
[node](https://nodejs.org) to run what you build.

Upgrade with `brew upgrade vilan`. The toolchain's own `vilan upgrade`
recognises a Homebrew install and says so rather than overwriting a file
Homebrew owns.

For Windows, or for an install that `vilan upgrade` maintains itself, use the
[install script](https://github.com/vilan-lang/vilan#getting-started)
instead.

## How this repository is maintained

`Formula/vilan.rb` is **generated, never hand-edited**. It is the output of
`scripts/brew-formula.sh <version> sha256sums.txt` in the
[Vilan repository](https://github.com/vilan-lang/vilan), run against the
checksum file the GitHub Release publishes beside its archives; that
repository's release workflow re-runs it on every tag and pushes the result
here as one commit, `vilan <version>`.

So the formula is a pinned pointer at one release: four `url`s under that
tag's download path, four `sha256`s taken from the release's own
`sha256sums.txt`. A fix belongs in the script — a hand-edit here is
overwritten by the next release, and the Vilan repository's test suite
regenerates the formula and compares it byte-for-byte with the copy staged
under `homebrew/` there.

## License

MIT OR Apache-2.0, at your option — the same terms as
[Vilan itself](https://github.com/vilan-lang/vilan/blob/main/LICENSE-MIT),
which is what the formula installs.
