# Spec §2 — Lexical structure

Source text is UTF-8. Lexing converts it to a token stream; **trivia**
(whitespace and comments) separates tokens and is otherwise discarded. A
file consisting only of trivia lexes to an empty token stream (and parses
as an empty module).

**Line terminators.** A line terminator is `\n`, and the two-character
sequence `\r\n` is **one** line terminator. Text built from source (the
value of any string literal) is built from the normalized text, so a
literal spanning lines carries `\n` per source line break and never
`\r\n`, whatever the file's on-disk encoding. A lone `\r` (no following
`\n`) is not a line terminator: between tokens it is ordinary whitespace,
and inside a string literal it is an ordinary character, preserved. A
leading U+FEFF byte-order mark is an encoding marker rather than source
text: it is ignored, and positions are counted from the byte after it. A
U+FEFF anywhere else is content. Together these make a program's meaning
independent of how an editor saved it: the same source is the same
program on every platform. (See [the design notes](https://github.com/vilan-lang/proposals/blob/main/proposal/windows-support.md) §2; the
canonical on-disk form is LF with no BOM, and `vilan fmt` writes it.)

## 2.1 Comments

A comment begins with `//` and runs to the end of the line. There are no
block comments.

## 2.2 Identifiers and keywords

```text
IDENT = ascii_letter | "_" , { ascii_letter | digit | "_" } ;
```

Identifiers are ASCII. The following words are **reserved**; they lex as
keyword tokens and are never `IDENT`:

```text
async     await  borrows  const  css   else    enum  export
external  for    fun      if     impl  import  in    is
jump      let    macro    match  mod   mut     null  own
resource  ret    struct   trait  type  use     with  true
false
```

(`true`/`false` lex as boolean literals; `null` as the null literal.)

**Contextual keywords** lex as `IDENT` and take meaning only by position:
`context` (the clause after a closure type, §3.9), `sync` (the marker
opening a closure type, §3.9), `void` (the unit value/type), `self` and
`Self` (receiver and receiver type), `derive`, `service`, `extern`,
`must_use`, `rpc`, `trait_only`, `doc`, `expose`, `platform`,
`deprecated` (attribute names in `[...]` position), and jump targets
(`break`, `continue`) after `jump`. All remain usable as ordinary
identifiers elsewhere.

## 2.3 Literals

### Numbers

```text
NUMBER = decimal | hex ;
decimal = digits , [ "." , digits ] , [ SUFFIX ] ;
hex     = "0x" , hexdigit , { hexdigit } , [ SUFFIX ] ;
SUFFIX  = IDENT   (* immediately adjacent, no space *)
```

The suffix names the literal's type: `i8 i16 i32 u8 u16 u32` (that
two's-complement width), `i53`/`u53` (the wide integers; see below),
`f` (`f64`), `f32`, `f64`, `n` (`BigInt`). An **unknown suffix is a
compile error** (the retired `i64`/`u64` suffixes get a rename hint). An
unsuffixed integer literal is `i32`; an unsuffixed fractional literal is
`f64`. Every integer literal is **range-checked** against its type at
compile time.

`i53` spans the symmetric range ±2^53 and `u53` spans [0, 2^53]: the
window in which every integer is exactly representable in an IEEE-754
double (the backing representation). The names deliberately follow
JavaScript's safe-integer convention (53 bits of integer precision)
rather than the two's-complement `iN` convention. There is no `i64`;
integers beyond the window take `BigInt`.

In a hex literal the digit run is maximal, so a suffix must begin with a
non-hex letter: `0xFFu8` is valid; `0xFFf` is a single hex number `0xFFF`,
not `0xFF` with suffix `f`.

### Strings

```text
STRING           = '"' , { string_char } , '"' ;
string_char      = "\" any_char_except_line_terminator
                 | any_char_except_quote_backslash_or_line_terminator ;
MULTILINE_STRING = '"""' , raw_text , '"""' ;
```

In a plain string a backslash escapes the next character; escape sequences
are preserved in the token and interpreted at code generation with
JavaScript string-escape semantics (`\n`, `\"`, `\\`, …). A multiline
string is **raw** (a backslash is a backslash) and runs to the first
`"""`; the whitespace prefix of the line containing the closing delimiter
is stripped from every line of the content.

A single-quoted string **must close on the line it opens**. A raw line
break inside `"…"` is an error, and so is a backslash immediately before
one: nothing escapes a line terminator, because the literal has to close
on its line either way. Multi-line text is written `"""…"""`; a single
line break inside a one-line string is written `\n`. (The rule buys error
locality: a forgotten closing quote is reported at its own line instead of
running on to the next `"` anywhere below it.)

In a multiline string a source line break contributes a single `\n` to the
value per the line-terminator rule above. An escaped `\r` is unaffected:
it is written into the literal, not read off the end of a line.

### Interpolated strings

```text
ISTRING          = 'i' , '"' , { istring_part } , '"' ;
istring_part     = hole | istring_char ;
hole             = '{' , expression , '}' ;
istring_char     = "\" any_char_except_line_terminator
                 | any_char_except_brace_quote_backslash_or_line_terminator ;
```

`i"…"` is an interpolated string: `{expr}` holes embed expressions; `\{`
and `\}` are literal braces. Every other escape is carried through as the
plain twin carries it — `\` and the character it precedes, interpreted at
code generation — and an unescaped `}` outside a hole is an error. The
construct is defined by desugaring. An interpolated string is exactly
equivalent to a parenthesized concatenation:

```text
i"Hello, {name}!"   ≡   ("" + "Hello, " + (name) + "!")
```

Each hole's contents are lexed as ordinary tokens (except that `{`/`}`
delimit the hole; string literals inside a hole may still contain braces)
and parsed as a single parenthesized expression. The result of the whole
form is `str`; each part must therefore be valid as a `+` operand with
`str` (§5's operator dispatch).

`i"…"` obeys the single-line rule of its plain twin: a raw line break in
its body (or a backslash before one) is the same error. Interpolated
multi-line text is `i"""…"""`, which is how a macro writes the code it
returns.

#### Interpolated multiline strings

```text
INTERPOLATED_MULTILINE = 'i' , '"""' , raw_text , '"""' ;
```

`i"""…"""` is the multiline string with holes. Two rules apply, in this
order:

1. **Trimming, on the literal's raw text.** The layout rule of a plain
   `"""` applies unchanged and applies *first*: nothing may follow the
   opening delimiter on its line, the closing delimiter sits alone on its
   line, and the whitespace preceding it is the indentation prefix
   stripped from the start of every content line. Holes and `\{` / `\}`
   count as ordinary characters of that text, so a hole never disturbs its
   line's indent accounting: a line opening with a hole is indented like
   any other, and a hole in the middle of a line has no effect on it. A
   hole may span lines; its continuation lines carry the prefix like every
   other line, and stripping is a no-op inside the hole, where whitespace
   is trivia.
2. **Fragmenting, on the trimmed text.** Exactly two escapes exist: `\{`
   and `\}`, each a literal brace. Nothing else is an escape: a backslash
   before any other character is a literal backslash and that character,
   the same near-rawness as a plain `"""` (`\n` is a backslash and an
   `n`). An unescaped `}` outside a hole is an error, as it is in `i"…"`.

The body is raw and runs to the first `"""`, so a single `"` and a `""`
pair are ordinary content. The value is a `str`, and (this being one of
the two forms that may span lines) a source line break in it contributes
one `\n`.

```vilan,fragment
let report = i"""
    {name} scored {score}.
    Braces are written \{like this\}.
    """
```

*Implementation note: because a hole is re-lexed as ordinary tokens, a
string literal inside a hole cannot use `\"` escapes; nested quoting
inside holes is currently a parse error. Bind the value to a local first.*

### Other literals

`true`, `false` (type `bool`); `null` (the host-boundary null, §5.2);
`void` (the unit value; a contextual identifier, not a keyword).

## 2.4 Operators and punctuation

Two token classes:

- **Operator tokens**: a maximal run of the characters `- : ! * / + = | &
  ^ ? %`, plus the arrow `=>` (lexed as one token). Maximal munch means
  `==`, `!=`, `+=`, `::`, `?.`'s `?`, `&&`, `||` each lex as single
  operator tokens; conversely `a+-b` lexes as `a`, `+-`, `b` and is a parse
  error.
- **Control tokens**: the single characters `( ) [ ] { } < > ; , .`.

`<` and `>` are control tokens (they delimit generics), not operator
characters. Consequently `<=`/`>=` lex as `<`/`>` followed by `=`, and the
shift operators `<<`/`>>` are two adjacent control tokens; the parser
accepts them as shifts only when **span-adjacent** (no whitespace):
`a << b` is a shift, `a < < b` is a parse error (§3.7). The element form
(grammar spec, *atom*) reuses the same discipline: `</` and `/>` are
span-adjacent pairs, `on:` joins by adjacency, and a hyphenated element
or attribute name (`aria-label`, `my-widget`) is a span-adjacent
name-`-`-name run — `data - id` is three tokens of arithmetic, not a
name. The lexer itself is untouched by the element form: every token it
produces already existed. The `css` block (grammar spec, *atom*) is the
same: a property name is that identical span-adjacent run, and a
dimension such as `1px` or `1.5rem` was already one number token (§2.3),
so CSS's own value shape needed nothing new.

**`#` and `@` are in neither class, and lex as nothing at all.** They
belong to no run and open no literal, so either one is a lex error
wherever it is written — including inside a `css` block, which cannot
change that, because lexing finishes before any parser exists (§2.5).
The two consequences are stated rather than worked around, and each
diagnostic names its spelling: a colour is a hole,
`color: {Color::hex("#333")};`, which routes the value through the
`Color` type that carries its own `:root` line; and a block has no
at-rules, so a media query is a breakpoint condition rule
(`.md { … }`) and a declaration block under a selector of your own is
`std::style::declare`. `#id` selectors are unwritable for the same
reason; `[id="x"]` is the spelling.

## 2.5 Trivia and token separation

Whitespace (any Unicode whitespace) and comments may appear between any
two tokens and are required only where two tokens would otherwise lex as
one (`fun main` needs the space; `a + b` does not). Lexing is greedy and
context-free: no token depends on parse state — which is why a construct
whose body reads as another language, such as the `css` block, still
lexes as ordinary vilan tokens and admits not one byte more.
