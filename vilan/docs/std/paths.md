# Paths reference

`std::path` is path arithmetic over plain `str`: `join`, `basename`,
`dirname`, `extname`, `stem`, `normalize`, `resolve`, `relative` and a
component-wise `starts_with`. It is pure vilan with no host call in it, so it
is **available on every platform** — a browser build routes URL paths, an SSR
build routes the same ones — and it is **const-evaluable**, so a path built
from literals folds at compile time.

```vilan,fragment
fun is_absolute(path: str): bool
fun normalize(path: str): str
fun join(base: str, part: str): str
fun join_all(parts: List<str>): str
fun resolve(base: str, path: str): str
fun basename(path: str): str
fun dirname(path: str): str
fun extname(path: str): str                          // WITH the leading dot
fun stem(path: str): str                             // basename minus extname
fun starts_with(path: str, prefix: str): bool        // by component, not by text
fun strip_prefix(path: str, prefix: str): Option<str>
fun relative(from: str, to: str): Option<str>
```

```vilan
import std::print;
import std::path;

fun main() {
	print(path::join("src/static", "app.css"));   // src/static/app.css
	print(path::basename("/var/log/app.log"));    // app.log
	print(path::dirname("/var/log/app.log"));     // /var/log
	print(path::extname("/var/log/app.log"));     // .log
	print(path::stem("/var/log/app.log"));        // app
	print(path::normalize("a/b/../c/"));          // a/c
	print(path::resolve("/srv/site", "../etc"));  // /srv/etc
}
```

## There is no `Path` type, deliberately

`std::fs` takes and returns plain `str` paths, and so does everything built on
it. A `Path` type would have to convert at every one of those call sites or
change all of them, and the two functions that would justify it — `normalize`
and `relative` — are a dozen lines each as free functions. So a path is a
`str`, there is one representation of it in the language rather than two, and
nothing here forecloses a wrapper later.

## The separator is `/`, on every platform

Paths are POSIX-shaped everywhere. This is a correctness rule, not a Unix
preference: a `join` that emitted `\` on Windows would make every path a
program derives — a cache key, an asset URL, a golden file — differ by host,
and Windows accepts `/` in ordinary filesystem APIs anyway. What is genuinely
not modelled is said plainly rather than half-supported:

- **Drive-absolute (`C:/…`) and UNC (`\\server\share`) paths are not roots.**
  `is_absolute("C:/x")` is `false`, and `\` is an ordinary character (it is a
  legal filename byte on Linux and macOS, so reading it as a separator would
  be wrong on the platform with the most files).
- **`fs::read_dir_all` returns HOST-separator paths.** On Windows its entries
  are backslash-joined, and `std::path` reads such an entry as one component.
- **Nothing here folds case, ever.** `normalize` preserves the case it was
  given and every comparison is byte-for-byte, so `/A` and `/a` are different
  paths.

## `normalize` is canonical, which means the trailing separator goes

```vilan
import std::print;
import std::path;

fun main() {
	print(path::normalize("a//b/./c"));                    // a/b/c
	print(path::normalize("a/b/../c"));                    // a/c
	print(path::normalize("a/b") == path::normalize("a/b/"));  // true
	print(path::normalize(""));                            // .
	print(path::normalize("/../a"));                       // /a — the root's parent is the root
	print(path::normalize("../a"));                        // ../a — a relative climb is kept
}
```

Node's `path.normalize` keeps a trailing separator, so its `"a/b"` and
`"a/b/"` stay unequal; this one drops it, because the whole point of a
canonical form is that two spellings of one path compare equal. That and
`dirname("a//b")` (`"a"` here, `"a/"` in node) are the only two places these
functions answer differently from `path.posix`.

Everything is **lexical** — nothing is read from disk, so a `..` cancels
against the name before it even when that name is a symlink pointing
somewhere else.

## `join` concatenates; `resolve` follows a reference

```vilan
import std::print;
import std::path;

fun main() {
	print(path::join("/a", "/b"));      // /a/b — an absolute part does NOT reset
	print(path::resolve("/a", "/b"));   // /b   — a reference to an absolute path is that path
	print(path::join("", "b"));         // b    — an empty side contributes nothing
}
```

`resolve` takes its base **explicitly**. Node's `path.resolve` falls back to
`process.cwd()`; taking a working directory would have made this whole module
`@process`-only for one function's convenience, and a caller who means
"against the working directory" can pass it.

## `extname`: a dotfile has no extension

`extname(".gitignore")` is `""`, not `".gitignore"`. The leading dot marks the
file as hidden — it does not name a type — and getting this wrong is the most
common bug in hand-rolled path code.

```vilan
import std::print;
import std::path;

fun main() {
	print(path::extname("index.html"));   // .html
	print(path::extname("a.b.c"));        // .c — only the last dot counts
	print(path::extname("noext"));        // (empty)
	print(path::extname(".gitignore"));   // (empty)
	print(path::extname("a.b/c"));        // (empty) — a dot in a directory is not the file's
	print(path::stem("/a/b.txt"));        // b
}
```

`extname` includes the leading dot, and `stem(p) + extname(p) == basename(p)`
holds for every `p`.

## `starts_with` compares components, `str::starts_with` compares text

```vilan
import std::print;
import std::path;

fun main() {
	print("/a/bc".starts_with("/a/b"));           // true  — about the TEXT
	print(path::starts_with("/a/bc", "/a/b"));    // false — about the FILESYSTEM
	print(path::starts_with("/a/b/c", "/a/b"));   // true
}
```

`/a/bc` is not inside the directory `/a/b`, and cutting a textual prefix to
derive a key or an asset URL is the same mistake in the other direction.

`starts_with` tests and `strip_prefix` cuts, exactly as the two `str` verbs
divide the same work on text:

```vilan
import std::print;
import std::path;
import std::option::{ Some, None };

fun main() {
	print(path::strip_prefix("/srv/site/css/app.css", "/srv/site").unwrap_or("not under"));  // css/app.css
	print(path::strip_prefix("/a/bc", "/a/b").unwrap_or("not under"));                       // not under
	print("/a/bc".strip_prefix("/a/b").unwrap_or("?"));                                      // "c" — the wrong tool
}
```

Cutting the whole of a path gives `Some(".")`. There is deliberately no path
`strip_suffix`: the only trailing affix a path commonly carries is a file
extension, and removing that is `stem`.

`strip_prefix` and `relative` answer different questions. `relative` always
has an answer for two paths under one root and will climb with `..` to reach
it; `strip_prefix` is "is this mine, and if so what do I call it", and says
`None` when it is not.

## `relative` inverts `resolve`

```vilan
import std::print;
import std::path;
import std::option::{ Some, None };

fun main() {
	match path::relative("/srv/site/css", "/srv/site/js/app.js") {
		Some(let hop) => print(hop),      // ../js/app.js
		None => print("no lexical answer"),
	}
}
```

`resolve(from, relative(from, to))` is `normalize(to)`. The result is `None`
in the two cases with no lexical answer: `from` and `to` disagree about being
absolute (there is no working directory here to bridge them), or `from` still
begins with `..` after the common prefix comes off — climbing out of an
unknown place, so what is above it is unknown too.

## URL paths are out, and the seam is the query string

These functions are for paths, not URLs. They share the `/`-separated core and
nothing else: `extname("/app.js?v=2")` is `.js?v=2` under a file rule and
`.js` under a URL rule, `%2F` is not a separator, and a URL's root is an
origin rather than a filesystem root. The split is that a URL's own grammar —
scheme, authority, query, fragment — is cut first, and what is left is an
ordinary path these functions read. `std::document`'s internal `file_name_of`
is exactly that, in two lines:

```vilan,fragment
fun file_name_of(url: str): str {
	path::basename(url.split("?")[0].split("#")[0])
}
```
