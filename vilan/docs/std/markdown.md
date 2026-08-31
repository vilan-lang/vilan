# Markdown

`std::markdown` parses markdown to a plain-data AST. It is deliberately
not CommonMark: its grammar is the census of what the vilan book
actually writes ([design notes](https://github.com/vilan-lang/proposals/blob/main/proposal/markdown.md) §1), and it is **strict** — a
construct outside that grammar is a loud `ParseError`, never a silently
wrong render. It is also the first *package-shaped* std module: one
file, leaf imports, no compiler-known names, plain data end to end,
versioned with the toolchain until a package registry exists.

The parser is pure computation — no I/O, no platform types — so it runs
on any target, and every type it produces is const-eligible by
construction (`str`, `i32`, `bool`, `List`, structs, enums; no `Shared`,
no `View`, no closures). That eligibility is exercisable: with the
compile-time file channel, `const parse(asset::read("pages/intro.md"))`
parses a page during compilation — within the const fuel budget even
for the book's largest page — and ships the `Doc` as plain data in the
output, with the read file tracked as a build input
([std::asset](misc.md#stdasset)).

## Parsing

`parse` takes the source text and returns `Result<Doc, ParseError>`.
Decoding untrusted input is fallible the same way `from_json` is: handle
the `Err` with `match`, `!`, or `unwrap_or_else`.

```vilan
import std::markdown::{ parse, Block, Doc, Inline, ParseError };
import std::result::Result::{ Err, Ok };

fun main() {
	let source = "# Notes\n\nSome `inline code` and a [link](https://vilan-lang.org).\n";
	match parse(source) {
		Ok(let doc) => {
			for block in doc.blocks {
				match block {
					Block::Heading(let level, let content, let id) => {
						print("h" + level.to_string() + " #" + id);
					}
					Block::Paragraph(let content) => {
						print("paragraph, " + content.len().to_string() + " inlines");
					}
					Block::CodeFence(let info, let body) => {
						print("fence [" + info + "]");
					}
					Block::Quote(let inner) => {
						print("quote");
					}
					Block::Items(let ordered, let items) => {
						print("list, " + items.len().to_string() + " items");
					}
					Block::Table(let header, let rows) => {
						print("table, " + rows.len().to_string() + " rows");
					}
				}
			}
		}
		Err(let error) => {
			print(error.to_string()); // "line 3: a footnote ([^label]) — …"
		}
	}
}
```

## The AST

The whole public surface is plain data — two enums, two structs, two
functions:

```vilan,fragment
enum Inline {
	Text(str),
	Code(str),                    // span content, CommonMark-trimmed
	Strong(List<Inline>),
	Emph(List<Inline>),
	Link(str, List<Inline>),      // destination, label
	Html(str),                    // one verbatim tag: <a id="…">, </a>
}

enum Block {
	Heading(i32, List<Inline>, str),  // level 1–6, content, mdBook id
	Paragraph(List<Inline>),
	CodeFence(str, str),              // info string, verbatim body
	Quote(List<Block>),               // recursive
	Items(bool, List<List<Block>>),   // ordered?, items (each a block list)
	Table(List<List<Inline>>, List<List<List<Inline>>>),  // header, rows
}

struct Doc { blocks: List<Block> }
struct ParseError { line: i32, message: str }   // implements Display

fun parse(source: str): Result<Doc, ParseError>
fun heading_id(content: List<Inline>): str      // base id, dedupe-free
```

A list item is a `List<Block>`, not a line of inlines: the book's own
bullets carry multi-paragraph bodies and indented code fences, and the
AST represents them as they render. The renderer is deliberately *not*
in this module — walking `Doc` into views, HTML, or a link checker is
the consumer's code, which is what keeps the package platform-neutral.

## Strict by design

The grammar covers: ATX headings, backtick code fences with info
strings, paragraphs, flat lists (`-` and `1.`), blockquotes, pipe
tables (with `\|` cell escapes, no alignment), inline code, strong,
emphasis, inline links, `<https://…>` autolinks, and the one HTML
passthrough shape `<a id="…"></a>`.

Everything with a measured count of zero in the book is out by decision,
and refused with an error naming the construct and its line: images,
footnotes, strikethrough, reference-style links and their definitions,
setext headings, indented code blocks, thematic breaks, nested lists,
hard line breaks, backslash escapes (beyond `\|` in a cell), tilde
fences, custom heading ids, lazy continuations, and raw HTML beyond the
anchor shape. The point of the refusal is the docs gate: the first page
to write a footnote fails the suite loudly instead of rendering wrong.

```vilan
import std::markdown::{ parse, Doc, ParseError };
import std::result::Result::{ Err, Ok };

fun main() {
	match parse("some ~~struck~~ text\n") {
		Ok(let doc) => print("unreachable — strikethrough is out of grammar"),
		Err(let error) => print(error.message),
	}
}
```

## Heading ids

Heading ids are a compatibility surface, not a rendering choice: the
book's URL space is mdBook's `page.html#slug`, the LSP's keyword hovers
deep-link into it, and any renderer that ever replaces mdBook must
reproduce those ids exactly. So `parse` computes each heading's id with
mdBook v0.5.4's algorithm — pinned by a unit corpus and by a book-wide
golden that walks every page of this book on every suite run.

The algorithm, measured rather than guessed: take the heading's text
(code-span content kept, emphasis markers gone, link labels kept, HTML
tags dropped, the result trimmed), lowercase it, keep alphanumerics with
`-` and `_`, turn each whitespace character into its own `-`, and drop
everything else. Repeated ids within one document gain `-1`, `-2`, … in
order of appearance. The consequences are unintuitive enough to pin:

| heading (source) | id |
|---|---|
| `# Spec §1 — Introduction & conformance` | `spec-1--introduction--conformance` |
| ``## `Shared<T>`: one cell, many holders`` | `sharedt-one-cell-many-holders` |
| `# Macros & const` | `macros--const` |
| ``## Conversions: `as_*` `` | `conversions-as_` |
| ``## `macro { … }` blocks`` | `macro----blocks` |

`heading_id` exposes the base algorithm (no dedupe) for consumers that
need to predict an anchor — a link checker, a table of contents.

```vilan
import std::markdown::{ heading_id, Inline };

fun main() {
	mut content: List<Inline> = [];
	content.push(Inline::Text("Macros & const"));
	print(heading_id(content)); // macros--const
}
```

## The package shape

`std::markdown` is built as if published
([design notes](https://github.com/vilan-lang/proposals/blob/main/proposal/std-shape.md) §6):
one base-root module file, imports limited to Tier-1 core
(`option`, `result`, `display`), no name the compiler knows, a
plain-data public surface, this page and its test surface — per-census
construct pins, per-refusal strict pins, the anchor corpus, and the
book-wide golden. The spelling `std::markdown` is final under the
namespace model; only the file's home moves when a package registry
exists.
