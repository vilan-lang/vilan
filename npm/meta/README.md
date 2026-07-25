# vilan

The [vilan](https://github.com/vilan-lang/vilan) toolchain, installed by npm.

```sh
npm install -g @vilan-lang/vilan
vilan --version
```

That gives you two commands:

- **`vilan`** — the compiler and its dev loop: `vilan run`, `vilan build`,
  `vilan test`, `vilan fmt`, `vilan watch`, `vilan upgrade`.
- **`vilan-lsp`** — the language server, which the
  [VS Code extension](https://github.com/vilan-lang/vilan/tree/main/editors/vscode)
  starts for you.

**The book (start here): <https://vilan-lang.github.io/vilan/>** — the guided
tour, the language guide, the standard library reference and the specification.

## What this package is

vilan is a compiler written in Rust that emits JavaScript, so this package
delivers a native binary, not a JS implementation. It carries no code of its
own beyond two `bin` stubs; the binaries live in five per-platform packages
(`@vilan-lang/linux-x64`, `linux-arm64`, `darwin-x64`, `darwin-arm64`,
`win32-x64`), declared here as optional dependencies. npm reads their `os` and
`cpu` fields and installs exactly the one your machine can run — no download
step at install time, so it works behind a proxy, on an offline mirror and
under `npm ci`.

Running `vilan` runs that binary directly, with your arguments, your streams
and its exit code; the stub adds nothing.

`vilan upgrade` knows it is installed by npm and will point you back at
`npm update -g @vilan-lang/vilan` rather than overwriting files npm owns.

## Other ways to install

The [releases page](https://github.com/vilan-lang/vilan/releases) carries
checksummed archives for the same five targets, plus install scripts for shells
and PowerShell. See the repository's README.

## License

MIT OR Apache-2.0, at your option.
