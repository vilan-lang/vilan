# Native binaries

Vilan has a second backend. `--backend rust` emits your program as Rust source,
writes it into a cargo project under `dist/native/`, and builds it with the
`cargo` already on your machine:

```sh
vilan build --backend rust hello.vl   # writes dist/native/hello/ and builds it
vilan run   --backend rust hello.vl   # builds, then runs the binary
```

The program is the same program. The point of the backend is that its output is
a binary with no runtime host — no Node, no browser, no garbage collector — and
the way that is kept honest is a differential: every program the backend accepts
must print **byte-identical** output under both backends, and a gate in the
compiler's own suite proves it over the test corpus.

## This is a first cut, and it says what it cannot do

The native backend is the first slice of a longer arc, and it is deliberately
narrow. What it reaches today:

- structs, enums, `Option` and `Result`
- `str`, `List`, `Map`, `Set`
- closures, `impl` blocks, `match`, loops, recursion
- `print` and `panic`

What it does **not** reach yet — each refused by name, with the construct in the
message, rather than silently mis-compiled:

- **generic functions and generic types.** Monomorphisation is the next slice,
  and it is the single largest gap: most of what the standard library offers
  beyond the list above goes through one.
- **module-level bindings** (`let` at the top of a file).
- **`async`, `await`, `Task` and `Nursery`.** Natively these need an executor,
  which is designed but not built.
- **anything with a host binding** — `std::fs`, `std::http`, `std::db`,
  `std::fetch`, `std::dom`, `std::ui`, `std::rpc`, the whole platform surface.
- **`resource` types.** Deterministic teardown natively is its own slice.
- **`--watch`.** A native round is a full `cargo build`; the dev loop's
  hot-swap belongs to the JS backend.

If your program uses one of these, `--backend rust` tells you which and stops.
Build it with `--backend js`, which is still the default and still where every
shipping feature lives.

## Debug by default

`vilan run --backend rust` builds in **debug**, and so does `vilan build
--backend rust`. rustc is the inner loop from here on: a release build of a
seven-hundred-line program took over a second of CPU in the design probe, and a
real program is orders larger. When you want the optimised binary, build the
generated project yourself:

```sh
cargo build --release --manifest-path dist/native/hello/Cargo.toml
```

## Numbers

`i53` and `u53` are JavaScript-shaped types — the exact-integer range of a
double. Natively they are `i64` and `u64`, which is wider, but **the guarantee
is still the JavaScript one**: a value outside ±2^53 is outside what the
language promises, and a program that round-trips through both backends behaves
the same only inside that range. Every other numeric type is its own width on
both backends.

**`usize` is Rust's `usize` here** — the platform word, which is what a native
`len()` is — with the same JavaScript guarantee: `usize::max_value()` answers
`9007199254740992` on both backends. Subtracting one past zero is unspecified,
and the backends really do differ: JS prints the negative number, a native
debug build panics with `attempt to subtract with overflow`, a release build
wraps, and rustc refuses a subtraction it can see underflows at compile time. So
a program that underflows is not one this backend promises to print the same
bytes for, and the corpus's `usize-underflow.vl` is named outside the
differential for exactly that reason. `checked_sub` and `saturating_sub` answer
the same on both.

**`BigInt` is an `i128` here, and that is a limit rather than a promise.** On
the JS backend a `BigInt` is arbitrary precision; natively it is a 128-bit
signed integer — ±170,141,183,460,469,231,731,687,303,715,884,105,727 — because
the runtime this backend links against takes no dependencies and so has no
bignum to lower one to. The limit is enforced at both ends rather than papered
over: a literal past it is **refused at compile time**, naming the value and the
range, and an operation that leaves the range **traps at run time** with the
same sentence instead of wrapping to a wrong number. Inside the range the two
backends agree exactly, printing included — node writes a `BigInt` with its
suffix, so `7n / 2n` is `3n` on both. If you need arbitrary precision as the
SUBJECT of a program — RSA, a big factorial — that is the JS backend today.

**Integer overflow is the program's, on both backends.** A conforming program
does not overflow (spec §7.2a), and the two backends answer one that does
differently: the JS backend runs on with an out-of-range value, and a native
debug build panics. An overflow rustc can see at compile time — `a + 1` right
after `let a: i32 = 2147483647;` — does not build at all, and `vilan` says so as
the program's overflow, naming the expression rustc underlined in the emitted
Rust, rather than as a backend defect.

## Where the emitted Rust goes

`dist/native/<entry>/`, with `src/main.rs` the emitted program and `Cargo.toml`
pointing at the runtime crate. It is ordinary Rust and you can read it; that is
the point of emitting a language rather than machine code. `vilan build
--backend rust --stdout` prints it without building.

`rm -rf dist` means the same thing here as everywhere else.

## Not here yet

Native **windows and UI** are not in this backend and are not close: how UI code
should be written once and run on both native and the web is a design question
being settled first. Native **servers** — the `@process` family on this backend
— are the next thing after the gaps above are closed.
