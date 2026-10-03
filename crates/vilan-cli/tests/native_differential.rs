//! The native backend's exit test (tracker F1, slice S1a; `native-apps.md`
//! §5-S1): every program the Rust emitter ACCEPTS prints, as a native binary,
//! byte-identical stdout to the same program compiled by the JS backend.
//!
//! # Why this is the right gate
//!
//! It needs no new oracle. `ssr_differential` proves a second `std::ui`
//! implementation by compiling one program two ways and comparing bytes; this
//! is the same shape applied to a BACKEND rather than to a platform, and the
//! corpus it runs over is the one the whole project already trusts to say what
//! a program means (`vilan/test/`, whose rule is that a program terminates and
//! its output is its claim).
//!
//! # The three verdicts, and why a REFUSAL is one of them
//!
//! S1a's emitter is a first cut. Over the corpus, a program lands in exactly
//! one of three places:
//!
//! * **refused** — the emitter names a construct it does not reach (a generic
//!   function, a module-level binding, `async`, a host binding) and answers an
//!   error. Recorded, not failed: naming the gap is what turns it into S1b's
//!   work list, and [`the_refusals_are_named_and_counted`] prints the census.
//! * **identical** — compiled both ways, same bytes. The claim.
//! * **anything else** — a program the emitter accepted and then got wrong, or
//!   emitted Rust that rustc will not build. **That is a failure**, and it is
//!   the only thing this file lets through as one. A backend that quietly
//!   prints a different number is worse than a backend that refuses.
//!
//! # Cost
//!
//! rustc is ~1 s per program even in debug, so the default suite runs a handful
//! and `VILAN_NATIVE_DIFFERENTIAL=1` runs the whole corpus (the seal does). The
//! `vilan-rt` build is shared through one `CARGO_TARGET_DIR` under
//! `CARGO_TARGET_TMPDIR`, so the runtime compiles ONCE for the whole sweep
//! rather than once per program.
//!
//! # No assertion in this suite reads a clock (N116)
//!
//! Nothing here times anything, and that is deliberate — but a probe program
//! that SLEEPS is the same mistake wearing a different hat. The comparison is
//! over bytes, and if the ordering of those bytes depends on two deadlines
//! that a busy box can let expire together, the pin measures the box.
//!
//! So a probe whose claim is an ORDER states it with a gap no scheduling stall
//! on this machine closes: **the later deadline is at least ten times the
//! earlier one and at least 200 ms in absolute terms**, so the runtime has to
//! reach its timer phase at least that late before the two can tie. Where the
//! order is not the claim, a probe does not sleep at all. The alternative — a
//! virtual clock injected into `vilan-rt`'s deadline list — cannot reach here:
//! this suite compares two REAL processes, one of them `node`, and node's
//! clock is not ours to move.

use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const B473_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "trait Base<T> {\n",
    "\tfun name(self, value: T): str {\n",
    "\t\t\"the default\"\n",
    "\t}\n",
    "}\n",
    "\n",
    "trait Sub<T> with Base<T> {\n",
    "\tfun tag(self): i32;\n",
    "}\n",
    "\n",
    "trait Deeper with Sub<i32> {}\n",
    "\n",
    "struct Mine {}\n",
    "\n",
    "impl Mine with Sub<i32> {\n",
    "\tfun tag(self): i32 {\n",
    "\t\t7\n",
    "\t}\n",
    "}\n",
    "\n",
    "impl Mine with Base<i32> {\n",
    "\tfun name(self, value: i32): str {\n",
    "\t\ti\"the override {value}\"\n",
    "\t}\n",
    "}\n",
    "\n",
    "impl Mine with Deeper {}\n",
    "\n",
    "struct Plain {}\n",
    "\n",
    "impl Plain with Base<i32> {}\n",
    "\n",
    "impl Plain with Sub<i32> {\n",
    "\tfun tag(self): i32 {\n",
    "\t\t8\n",
    "\t}\n",
    "}\n",
    "\n",
    "trait Walk {\n",
    "\tfun next(&mut self): Option<i32> {\n",
    "\t\tNone\n",
    "\t}\n",
    "}\n",
    "\n",
    "trait SubWalk with Walk {}\n",
    "\n",
    "struct Count { n: i32 }\n",
    "\n",
    "impl Count with Walk {\n",
    "\tfun next(&mut self): Option<i32> {\n",
    "\t\tif self.n >= 3 {\n",
    "\t\t\tret None;\n",
    "\t\t}\n",
    "\t\tself.n += 1;\n",
    "\t\tSome(self.n)\n",
    "\t}\n",
    "}\n",
    "\n",
    "impl Count with SubWalk {}\n",
    "\n",
    "fun through_sub<S: Sub<i32>>(value: S): str {\n",
    "\tvalue.name(1)\n",
    "}\n",
    "\n",
    "fun through_deeper<S: Deeper>(value: S): str {\n",
    "\tvalue.name(2)\n",
    "}\n",
    "\n",
    "fun qualified<S: Sub<i32>>(value: S): str {\n",
    "\tSub::name(value, 3)\n",
    "}\n",
    "\n",
    "fun walk<W: SubWalk>(mut walker: W): i32 {\n",
    "\tmut total = 0;\n",
    "\tfor value in walker {\n",
    "\t\ttotal += value;\n",
    "\t}\n",
    "\ttotal\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tprint(through_sub(Mine {}));\n",
    "\tprint(through_deeper(Mine {}));\n",
    "\tprint(qualified(Mine {}));\n",
    "\tprint(Sub::name(Mine {}, 4));\n",
    "\tprint(through_sub(Plain {}));\n",
    "\tprint(walk(Count { n = 0 }));\n",
    "}\n",
);
const B467_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "struct Holder {\n",
    "\tf: |&mut str| void,\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet h = Holder { f = |&mut p| {\n",
    "\t\tp = \"direct ok\";\n",
    "\t} };\n",
    "\tmut s = \"old\";\n",
    "\t(h.f)(&mut s);\n",
    "\tprint(s);\n",
    "\tmut edits: List<|&mut List<i32>| void> = [];\n",
    "\tedits.push(|&mut list| list.push(7));\n",
    "\tprint(edits.len());\n",
    "}\n",
);
const RC_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::shared::Shared;\n",
    "import std::delta::ListCell;\n",
    "\n",
    "struct P { x: i32, tags: List<i32> }\n",
    "\n",
    "struct Address { city: str, zip: str }\n",
    "\n",
    "fun copy_out(v: &P): P {\n",
    "\tmut c: P = *v;\n",
    "\tc.x = 99;\n",
    "\tc.tags.push(5);\n",
    "\tc\n",
    "}\n",
    "\n",
    "fun returned(v: &P): P {\n",
    "\t*v\n",
    "}\n",
    "\n",
    "fun snapshot(v: &mut P): Option<P> {\n",
    "\tlet snap = Some(*v);\n",
    "\tv.x = 7;\n",
    "\tv.tags.push(8);\n",
    "\tsnap\n",
    "}\n",
    "\n",
    "fun with_city(a: Address, f: |&str| void) {\n",
    "\tf(&a.city);\n",
    "}\n",
    "\n",
    "fun with_flag(f: |&bool| void) {\n",
    "\tlet flag = true;\n",
    "\tf(&flag);\n",
    "}\n",
    "\n",
    "fun with_point(p: P, f: |&P| void) {\n",
    "\tf(&p);\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet xs: List<i32> = [1, 2];\n",
    "\tlet cell = Shared::new(xs);\n",
    "\tcell.write().push(3);\n",
    "\tprint(i\"shared {xs.len()} {cell.read().len()}\");\n",
    "\tlet ys: List<i32> = [1, 2];\n",
    "\tlet list = ListCell::of(ys);\n",
    "\tlist.push(3);\n",
    "\tprint(i\"list {ys.len()} {list.get().len()}\");\n",
    "\tlet zs: List<i32> = [1];\n",
    "\tlet limited = ListCell::with_limit(zs, 4);\n",
    "\tlimited.push(2);\n",
    "\tprint(i\"limited {zs.len()} {limited.get().len()}\");\n",
    "\tlet p = P { x = 1, tags = [] };\n",
    "\tlet c = copy_out(&p);\n",
    "\tprint(i\"deref {p.x} {p.tags.len()} {c.x} {c.tags.len()}\");\n",
    "\tmut r = returned(&p);\n",
    "\tr.tags.push(1);\n",
    "\tprint(i\"returned {p.tags.len()} {r.tags.len()}\");\n",
    "\tmut q = P { x = 2, tags = [] };\n",
    "\tlet snap = snapshot(&mut q);\n",
    "\tmatch snap {\n",
    "\t\tSome(let s) => print(i\"snapshot {s.x} {s.tags.len()} {q.x} {q.tags.len()}\"),\n",
    "\t\tNone => print(\"none\"),\n",
    "\t}\n",
    "\tlet home = Address { city = \"Oslo\", zip = \"1\" };\n",
    "\tmut out = \"\";\n",
    "\twith_city(home, |c| {\n",
    "\t\tout = *c;\n",
    "\t});\n",
    "\tprint(out);\n",
    "\tmut seen = false;\n",
    "\twith_flag(|b| {\n",
    "\t\tseen = *b;\n",
    "\t});\n",
    "\tprint(seen);\n",
    "\tmut total = 0;\n",
    "\twith_point(p, |point| {\n",
    "\t\ttotal = point.x;\n",
    "\t});\n",
    "\tprint(total);\n",
    "}\n",
);
const B474_PROBE: &str = concat!(
    "import std::json::json_codec;\n",
    "import std::io::print;\n",
    "import std::option::Option::{ None, Some, self };\n",
    "import std::reactive::{ Signal, SignalCell, Source };\n",
    "import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };\n",
    "\n",
    "fun through_the_trait<T, S: Source<T>>(source: S, observe: |T| void) {\n",
    "\tlet live = source.sub(|value| observe(value));\n",
    "\tlive.dispose();\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet (client_end, server_end) = duplex_pair();\n",
    "\tlet counter: SignalCell<i32> = Signal::new(9);\n",
    "\tlet channel = ReactiveServer::new(server_end, json_codec()).expose(counter);\n",
    "\tlet remote: RemoteSource<i32> = ReactiveClient::new(client_end, json_codec()).source(channel);\n",
    "\tthrough_the_trait(remote, |value: Option<i32>| match value {\n",
    "\t\tSome(let n) => print(i\"trait:{n}\"),\n",
    "\t\tNone => print(\"trait:none\"),\n",
    "\t});\n",
    "\tlet present = remote.sub(|value: i32| print(i\"inherent:{value}\"));\n",
    "\tcounter.set(10);\n",
    "\tpresent.dispose();\n",
    "}\n",
);
const B453_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun bump(slot: &mut i32) {\n",
    "\tslot += 10;\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tmut pair = (1, 2);\n",
    "\tlet v = &mut pair.1;\n",
    "\tv = 3;\n",
    "\tprint(pair.1);\n",
    "\tbump(&mut pair.0);\n",
    "\tprint(pair.0);\n",
    "\tmut triple: (str, i32, bool) = (\"a\", 5, false);\n",
    "\tlet flag = &mut triple.2;\n",
    "\tflag = true;\n",
    "\tprint(i\"{triple.0} {triple.1} {triple.2}\");\n",
    "}\n",
);
const B444_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "struct Box2 { n: i32, label: str }\n",
    "\n",
    "fun first(xs: &List<i32>): &i32 borrows xs {\n",
    "\t&xs[0]\n",
    "}\n",
    "\n",
    "fun label_of(b: &Box2): &str borrows b {\n",
    "\t&b.label\n",
    "}\n",
    "\n",
    "fun twice(n: i32): i32 {\n",
    "\tn * 2\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet xs = [7, 8];\n",
    "\tprint(i\"{first(&xs)}\");\n",
    "\tlet v = first(&xs);\n",
    "\tprint(i\"v={v}\");\n",
    "\tprint(v + 1);\n",
    "\tprint(twice(first(&xs)));\n",
    "\tprint(first(&xs));\n",
    "\tlet b = Box2 { n = 1, label = \"lab\" };\n",
    "\tprint(i\"{label_of(&b)}!\");\n",
    "\tprint(*v);\n",
    "}\n",
);
const B464_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "struct A { x: i32, s: str }\n",
    "\n",
    "fun apply(f: sync |&mut str| void) {\n",
    "\tmut a = A { x = 1, s = \"old\" };\n",
    "\tf(&mut a.s);\n",
    "\tprint(a.s);\n",
    "\tmut s = \"old\";\n",
    "\tf(&mut s);\n",
    "\tprint(s);\n",
    "\tmut xs = [\"old\"];\n",
    "\tf(&mut xs[0]);\n",
    "\tprint(xs[0]);\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tapply(|&mut p| {\n",
    "\t\tp = \"new\";\n",
    "\t});\n",
    "}\n",
);
const B504_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun show(v: &i32) {\n",
    "\tprint(*v);\n",
    "}\n",
    "\n",
    "fun show_str(v: &str) {\n",
    "\tprint(*v);\n",
    "}\n",
    "\n",
    "fun lend_match(held: Option<str>, f: |&str| void) {\n",
    "\tmatch held {\n",
    "\t\tSome(let payload) => {\n",
    "\t\t\tf(&payload);\n",
    "\t\t\tprint(payload);\n",
    "\t\t},\n",
    "\t\tNone => {},\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun lend_generic<T>(held: Option<T>, f: |&T| void) {\n",
    "\tmatch held {\n",
    "\t\tSome(let payload) => f(&payload),\n",
    "\t\tNone => {},\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlend_match(Some(\"ally\"), |&v: &str| print(*v));\n",
    "\tlend_generic(Some(\"ally\"), |&v: &str| print(*v));\n",
    "\tlend_generic(Some(7), |&v: &i32| print(*v));\n",
    "\tlet (a, b) = (4, \"five\");\n",
    "\tshow(&a);\n",
    "\tshow_str(&b);\n",
    "\tprint(a + 1);\n",
    "\tfor i in [6, 7] {\n",
    "\t\tshow(&i);\n",
    "\t\tprint(i * 10);\n",
    "\t}\n",
    "\tif Some(9) is Some(let nine) {\n",
    "\t\tshow(&nine);\n",
    "\t\tprint(nine);\n",
    "\t}\n",
    "\tmatch Some(10) {\n",
    "\t\tSome(let q) => {\n",
    "\t\t\tlet w = &q;\n",
    "\t\t\tprint(*w + q);\n",
    "\t\t},\n",
    "\t\tNone => {},\n",
    "\t}\n",
    "}\n",
);
const B506_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "struct Pair<T> {\n",
    "\tleft: T,\n",
    "}\n",
    "\n",
    "fun first<T>(xs: &List<T>): &T borrows xs {\n",
    "\t&xs[0]\n",
    "}\n",
    "\n",
    "fun left_of<T>(pair: &Pair<T>): &T borrows pair {\n",
    "\t&pair.left\n",
    "}\n",
    "\n",
    "fun lend<T>(pair: &Pair<T>, f: |&T| void) {\n",
    "\tf(&pair.left);\n",
    "}\n",
    "\n",
    "fun lend_through_binding<T>(pair: &Pair<T>, f: |&T| void) {\n",
    "\tlet v = &pair.left;\n",
    "\tf(v);\n",
    "}\n",
    "\n",
    "fun lend_parameter<T>(x: T, f: |&T| void) {\n",
    "\tf(&x);\n",
    "}\n",
    "\n",
    "fun read_back<T>(x: T): T {\n",
    "\tlet v = &x;\n",
    "\t*v\n",
    "}\n",
    "\n",
    "fun lend_transient<T>(x: T, f: |&T| void) {\n",
    "\tmatch Some(&x) {\n",
    "\t\tSome(let v) => f(v),\n",
    "\t\tNone => {},\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun bump_generic<T>(mut x: T, f: |&mut T| void): T {\n",
    "\tf(&mut x);\n",
    "\tx\n",
    "}\n",
    "\n",
    "fun write_left<T>(pair: &mut Pair<T>, f: |&mut T| void) {\n",
    "\tf(&mut pair.left);\n",
    "}\n",
    "\n",
    "fun relend<T>(x: &T, f: |&T| void) {\n",
    "\tf(&x);\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlend(&Pair { left = 1 }, |v: &i32| print(*v));\n",
    "\tlend(&Pair { left = \"ab\" }, |v: &str| print(*v));\n",
    "\tlend(&Pair { left = [2, 3] }, |v: &List<i32>| print(v.len()));\n",
    "\tlend_through_binding(&Pair { left = 4 }, |v: &i32| print(*v));\n",
    "\tlend_parameter(5, |v: &i32| print(*v));\n",
    "\tlend_parameter(true, |v: &bool| print(*v));\n",
    "\tprint(read_back(6));\n",
    "\tprint(read_back(\"seven\"));\n",
    "\tlend_transient(8, |v: &i32| print(*v));\n",
    "\tprint(bump_generic(9, |v: &mut i32| { v = *v + 1; }));\n",
    "\tprint(*first(&[11, 12]));\n",
    "\tprint(*left_of(&Pair { left = 13 }));\n",
    "\tmut pair = Pair { left = 1 };\n",
    "\twrite_left(&mut pair, |v: &mut i32| { v = *v + 41; });\n",
    "\tprint(pair.left);\n",
    "\tmut words = Pair { left = \"a\" };\n",
    "\twrite_left(&mut words, |v: &mut str| { v = *v + \"b\"; });\n",
    "\tprint(words.left);\n",
    "\trelend(&pair.left, |v: &i32| print(*v));\n",
    "}\n",
);
const B505_PROBE: &str = concat!(
    "import std::compare::PartialEq;\n",
    "import std::io::print;\n",
    "\n",
    "trait Same {\n",
    "\tfun same(&self, other: &Self): bool;\n",
    "}\n",
    "\n",
    "impl type T: PartialEq with Same {\n",
    "\tfun same(&self, other: &T): bool {\n",
    "\t\t*self == *other\n",
    "\t}\n",
    "}\n",
    "\n",
    "trait Bump {\n",
    "\tfun bump(&mut self);\n",
    "\tfun peek(&self): i32;\n",
    "}\n",
    "\n",
    "impl i32 with Bump {\n",
    "\tfun bump(&mut self) {\n",
    "\t\tself = *self + 1;\n",
    "\t}\n",
    "\tfun peek(&self): i32 {\n",
    "\t\t*self\n",
    "\t}\n",
    "}\n",
    "\n",
    "trait Flip {\n",
    "\tfun flip(&mut self);\n",
    "}\n",
    "\n",
    "impl bool with Flip {\n",
    "\tfun flip(&mut self) {\n",
    "\t\tself = !*self;\n",
    "\t}\n",
    "}\n",
    "\n",
    "struct Counter {\n",
    "\tcount: i32,\n",
    "}\n",
    "\n",
    "fun show(v: &i32) {\n",
    "\tprint(*v);\n",
    "}\n",
    "\n",
    "fun generic<T: Same>(a: T, b: T): bool {\n",
    "\ta.same(&b)\n",
    "}\n",
    "\n",
    "fun bump_twice<T: Bump>(mut x: T): T {\n",
    "\tx.bump();\n",
    "\tx.bump();\n",
    "\tx\n",
    "}\n",
    "\n",
    "fun peek_view(v: &i32): i32 {\n",
    "\tv.peek()\n",
    "}\n",
    "\n",
    "fun bump_view(v: &mut i32) {\n",
    "\tv.bump();\n",
    "}\n",
    "\n",
    "fun param(x: i32) {\n",
    "\tshow(&x);\n",
    "\tprint(x.peek());\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tprint(1.same(&1));\n",
    "\tprint(generic(\"x\", \"x\"));\n",
    "\tprint(generic(1, 2));\n",
    "\tlet a = 1;\n",
    "\tlet b = 1;\n",
    "\tprint(a.same(&b));\n",
    "\tmut x = 1;\n",
    "\tx.bump();\n",
    "\tprint(x);\n",
    "\tprint(x.peek());\n",
    "\tprint(7.peek());\n",
    "\tprint((3 + 4).peek());\n",
    "\tshow(&11);\n",
    "\tshow(&(a + 8));\n",
    "\tparam(12);\n",
    "\tlet apply = |n: i32| show(&n);\n",
    "\tapply(13);\n",
    "\tmut counter = Counter { count = 1 };\n",
    "\tcounter.count.bump();\n",
    "\tprint(counter.count);\n",
    "\tmut xs = [10, 20];\n",
    "\txs[0].bump();\n",
    "\tprint(xs[0]);\n",
    "\tmut pair = (5, \"p\");\n",
    "\tpair.0.bump();\n",
    "\tprint(pair.0);\n",
    "\tprint(bump_twice(7));\n",
    "\tprint(peek_view(&xs[1]));\n",
    "\tbump_view(&mut xs[1]);\n",
    "\tprint(xs[1]);\n",
    "\tmut flag = false;\n",
    "\tflag.flip();\n",
    "\tprint(flag);\n",
    "}\n",
);
const B496_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "struct P {\n",
    "\tx: i32,\n",
    "}\n",
    "\n",
    "struct Holder {\n",
    "\tp: P,\n",
    "}\n",
    "\n",
    "fun inner(holder: &Holder): &P borrows holder {\n",
    "\t&holder.p\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tmut a = P { x = 1 };\n",
    "\tmut out = P { x = 0 };\n",
    "\tout = *&a;\n",
    "\tout.x = 99;\n",
    "\tprint(a.x);\n",
    "\tlet holder = Holder { p = P { x = 2 } };\n",
    "\tout = *inner(&holder);\n",
    "\tout.x = 98;\n",
    "\tprint(holder.p.x);\n",
    "\tmut n = 1;\n",
    "\tlet m = 5;\n",
    "\tn = *&m;\n",
    "\tprint(n + 1);\n",
    "}\n",
);

/// The programs the default suite runs — small, fast, and between them they
/// cover every value shape S1a claims: scalars, `str`, `bool`, a struct with an
/// `impl`, an enum with a payload, a `List`, a `match`, a loop, recursion.
///
/// Between them they cover: `bool` and `match`, recursion and mutual
/// recursion, loops, rule 1's copies and the elisions the last-use pass makes,
/// a struct with an `impl` and a field write, iteration over a `List`, `Option`
/// through `List::get`/`remove`, `Display`'s interpolation, an enum payload
/// capture, and `&`/`&mut` views through a `borrows` return.
///
/// `board.vl`, the paper's own probe, is deliberately NOT here: it does not
/// compile natively yet, and [`the_probes_board_program_names_its_own_gap`]
/// pins WHY rather than pretending otherwise.
///
/// S1b adds six rows, one per thing the slice built: monomorphisation of a
/// generic function over two instantiations (`generic-inference.vl`), a
/// generic parameter's own defaulted bound (`default-generic-param.vl`), a
/// trait default specialized per type plus an inferred return type
/// (`default.vl`), `mut` parameters (`mut-parameters.vl`), a module-level
/// binding read and WRITTEN plus B105's hoist (`compound-index.vl`), and a
/// string literal's escapes (`interpolated-multiline-string.vl` — the class
/// S1a got wrong for every escape there is).
///
/// F18 slice 2 adds the two JSON rows: `derive-json.vl` (a `[derive(Json)]`
/// struct in both directions, a nested struct, a missing field and a
/// wrong-typed one) and `json-roundtrip.vl` (the `List`/`Option` blankets,
/// which reach a scalar's `[extern("JSON.stringify")]` member through a
/// generic dispatch). They were refused for four separate reasons before
/// `vilan_rt::json` existed, and every one of them is a row of the slice: the
/// host type, the six intrinsics, the `!` assertion the derived decoders are
/// written in, and a capturing `is`-test to the left of `&&`.
///
/// F32 adds the two `BigInt` rows. `remainder.vl` and `numeric-types.vl` are
/// the corpus's only `n` literals, and both were refused by name after Order
/// 39 caught the narrowing miscompile behind them (`9007199254740993n` had been
/// emitting `…993i32`). They print `1n` and `3n` on both backends now, which is
/// the whole of what the `i128` ruling claims.
///
/// F33 adds `bytes-aliasing.vl`: the mutable `Bytes` the ruling chose, read
/// through every kind of alias a program can make.
///
/// A124 R3 adds `dyn-objects.vl`: trait objects on both backends — a
/// heterogeneous `List` behind a struct field, a supertrait member and a
/// parameterized trait through the table, a blanket and a generic over the
/// bound, printing a struct that holds one (`[ value, {} ]` on both sides),
/// and a cold node over a `dyn Source<i32>` notified through its upstream.
const DEFAULT_SUITE: &[&str] = &[
    "bool.vl",
    "recursion.vl",
    "loops.vl",
    "copy-elision.vl",
    "field-assignment.vl",
    "for-in.vl",
    "list-splice.vl",
    "display.vl",
    "match-ergonomics.vl",
    "borrows.vl",
    "generic-inference.vl",
    "default-generic-param.vl",
    "default.vl",
    "mut-parameters.vl",
    "compound-index.vl",
    "interpolated-multiline-string.vl",
    "derive-json.vl",
    "json-roundtrip.vl",
    "remainder.vl",
    "numeric-types.vl",
    // F21: a VIEW inside an enum payload (`Option<&mut T>`) — the shape behind
    // a view-returning `Arena::get`, refused by name since native-b-38. Every
    // projection in it is matched where it is built, which is the position the
    // payload view is carried through.
    "option-view.vl",
    "dyn-objects.vl",
    // I5 S1: `usize`, the index type — the platform word natively, `u53` on JS,
    // printing the same family surface on both.
    "usize.vl",
    // I5 S1 + B389: the literal law at `usize`, all 21 measured positions in one
    // program — the literals' native widths are the record B389 writes.
    "usize-literals.vl",
    // I6 (RULED): a negative converted to `u53`/`usize` saturates to 0 — the
    // JS clamp against Rust's own `f64 as u64`, every signed source width.
    "unsigned-saturate.vl",
    // F33 (RULED (a)): `Bytes` is one shared, mutable buffer behind every
    // holder — two bindings, a parameter, a struct copy, a list and a closure
    // capture all write into the same bytes, as a `Uint8Array` does; `slice`
    // and `concat` are the two that make a new one.
    "bytes-aliasing.vl",
    // F18 slice 3: `ListCell`'s writes through `SignalCell::update(|&mut
    // list| ..)` and a `Delta` pushed with a parameter its payload leaves open
    // — refused at the Order 40 seal, byte-identical now.
    "delta-law.vl",
    // F34: `map` (a default's own generic bound from its closure argument) and
    // `flatten` (a `?` lift over an `Option`), both refused at the Order 40
    // seal.
    "reactive-on-change.vl",
    "reactive-flatten.vl",
    // F35: `ListCell`, the two sequence traits and `map_each` — with a named
    // function handed where a closure stands, a trait default writing through
    // `&mut self` while reading it, a `&mut` view forwarded, and the 300-turn
    // walk whose `edited[next_random(size)] = next_random(100)` is the
    // assignment-order pin's corpus witness.
    "list-cell.vl",
    // F36: `i"{one.get()}"` — a generic call's result, interpolated, judged at
    // the type the call binds its parameter to.
    "reactive-selector.vl",
    // N106: `vilan_rt::js_number`'s four documented departures from Rust's
    // own `{}` — 1e21's exponential switch, the 1e-6 linear floor, and the
    // two infinities — pinned identical on a corpus program rather than left
    // unrecorded (negative zero is the one departure that is NOT identical;
    // it is `f64-print-negative-zero.vl`, outside this differential by name).
    "f64-print-boundary.vl",
    // N106: `str::len` counts UTF-16 code units to match JavaScript's
    // `.length`; a character outside the BMP is a surrogate pair on JS and
    // `char::len_utf16` counts it the same way natively — verified here
    // rather than left as an unverified claim in `vilan_rt::str_len`'s doc
    // comment.
    "non-bmp-string-length.vl",
    // B414 S4: every reserved word names a member — fields, methods, a trait
    // default, a `::` static, a lifted link and a `[derive(Json)]` round trip
    // over a `"type"` key — and the Rust emitter's own keywords (`type`,
    // `match`, `if`, `in`, `for`, `ret`) must survive as member names natively.
    "keyword-members.vl",
    // F62: `a.write().count = a.write().count + 1` — a field read and written
    // through a `Shared` view, refused by name until the read became a scoped
    // borrow and the place's statement settled its value first.
    "shared.vl",
    // F83: `cell.write() += 1` — a compound write through a counted cell, as
    // a statement, in a closure, on a field, in an effect run inline and from a
    // turn drain, and on a captured and a module-level `mut`. Every one died
    // natively with "a cell was read while it is being updated" until the
    // value was settled before the cell's `set` took its borrow.
    "shared-compound-write.vl",
];

/// Corpus programs that are OUTSIDE this differential by construction, named
/// with the reason (I5 ruling 2, `proposal/index-type.md` §5.2).
///
/// Not refusals and not breakages: programs whose output the language leaves
/// UNSPECIFIED, so the two backends legitimately print different things and
/// "identical" is not a claim either could make. An underflowing `usize`
/// subtraction goes negative on JS and panics in a native debug build; the
/// corpus keeps its JS golden, and
/// [`an_underflowing_usize_is_outside_the_differential_by_name`] pins the
/// native half.
const OUTSIDE_THE_DIFFERENTIAL: &[(&str, &str)] = &[
    (
        "usize-underflow.vl",
        "a `usize` subtracted past zero is unspecified: -1 on JS, a debug panic natively",
    ),
    (
        "f64-print-negative-zero.vl",
        "negative zero prints \"-0\" through node's `console.log` (a `util.inspect` \
         special case) but \"0\" through every JS stringification `vilan_rt::js_number` \
         implements instead (`String(x)`, a template literal, `JSON.stringify`) — N106",
    ),
];

/// The corpus's ASYNC programs (tracker J6, lane native-b-38).
///
/// They are enumerated by name rather than reached through
/// [`platform_free_programs`] because `std::task` and `std::time` are still
/// rows of [`PLATFORM_MODULES`] — the executor gives the backend an event loop,
/// not a `std::time` twin (`now_millis`, `Date`, `sleep_for`'s `Duration` path
/// still bind the host). So this list is what J6's exit is measured against,
/// and every program on it lands in one of the same three verdicts the corpus
/// sweep uses: identical, refused by NAME, or broken.
const ASYNC_SUITE: &[&str] = &[
    "async-await.vl",
    "async-promise-all.vl",
    "await-postfix.vl",
    "adapt.vl",
    "nursery.vl",
    "reactive-turns.vl",
    "time.vl",
];

/// F57: the corpus programs that reach a platform module and that the
/// backend BUILDS — the ones the platform-free sweep leaves out by
/// construction, so no gate saw them. `crypto.vl` was rustc E0382 at the
/// Order 44 seal (a host binding's by-value argument moved its `salt`) behind
/// an E0308 (a `match` literal pattern written at the arms' width over a
/// `usize`), and nothing ran it. [`every_platform_bound_program_is_identical_or_named`]
/// requires these four and classifies the rest.
const PLATFORM_BOUND_REQUIRED: &[&str] =
    &["crypto.vl", "db.vl", "asset_bundle.vl", "element-syntax.vl"];

/// Platform-bound corpus programs whose stdout the two `vilan run`s cannot
/// agree on for a reason that is not the program's, named with the reason.
const PLATFORM_BOUND_OUTSIDE: &[(&str, &str)] = &[(
    "estate.vl",
    "the JS `vilan run` prints its build's asset report (`Bundled  robots.txt`, ...) on \
     STDOUT ahead of the program's output, and the native run reports nothing; the \
     program's own three lines are identical",
)];

/// Modules whose presence in an `import` means the program reaches a platform
/// surface S1a has none of. Written as a support list so Order 38 widens the
/// corpus by deleting rows rather than by rewriting the walk.
const PLATFORM_MODULES: &[&str] = &[
    "std::dom",
    "std::fetch",
    "std::fs",
    "std::http",
    "std::db",
    "std::rpc",
    "std::ui",
    "std::web",
    "std::storage",
    "std::router",
    "std::dev",
    "std::canvas",
    "std::process",
    "std::asset",
    "std::build",
    "std::task",
    "std::time",
    "std::crypto",
    "std::random",
];

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/test")
}

fn std_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std")
}

fn runtime_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../vilan-rt")
}

/// One shared cargo target directory for the whole sweep, so `vilan-rt` is
/// built once. Under `CARGO_TARGET_TMPDIR`, which cargo gives every integration
/// test and cleans with the rest of `target/`.
fn shared_target() -> PathBuf {
    let shared = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-differential");
    std::fs::create_dir_all(&shared).expect("create the shared cargo target directory");
    shared
}

/// Every platform-free corpus program, in a stable order.
///
/// **The enumeration is a support function on purpose** (the brief's own
/// instruction): Order 38's widening of the native corpus is an edit to
/// [`PLATFORM_MODULES`] and to what the emitter accepts, never a second walk
/// written beside this one that can disagree with it about what the corpus is.
pub fn platform_free_programs() -> Vec<String> {
    let mut programs = Vec::new();
    let entries = std::fs::read_dir(corpus_dir()).expect("read the corpus directory");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "vl") {
            let source = std::fs::read_to_string(&path).expect("read a corpus program");
            let reaches_a_platform = PLATFORM_MODULES
                .iter()
                .any(|module| source.contains(&format!("import {module}")));
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let outside = OUTSIDE_THE_DIFFERENTIAL
                .iter()
                .any(|(excluded, _)| *excluded == name);
            if !reaches_a_platform && !outside {
                programs.push(name);
            }
        }
    }
    programs.sort();
    programs
}

/// Every corpus program that reaches a platform module and is not one of
/// [`ASYNC_SUITE`]'s (which has its own gate), in a stable order — the
/// complement of [`platform_free_programs`] over the same walk.
fn platform_bound_programs() -> Vec<String> {
    let free = platform_free_programs();
    let mut programs = Vec::new();
    let entries = std::fs::read_dir(corpus_dir()).expect("read the corpus directory");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "vl") {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let outside = OUTSIDE_THE_DIFFERENTIAL
                .iter()
                .chain(PLATFORM_BOUND_OUTSIDE)
                .any(|(excluded, _)| *excluded == name);
            if !free.contains(&name) && !ASYNC_SUITE.contains(&name.as_str()) && !outside {
                programs.push(name);
            }
        }
    }
    programs.sort();
    programs
}

/// A staged copy of the corpus, so a build writes its artifacts beside a copy
/// rather than into the tree. Per CALL — one per test, since every test stages
/// once. nextest runs each test of this binary in its own process, concurrently,
/// and a staging directory the four shared — each one beginning by removing it —
/// was a race the Order 37 seal lost (`cannot create ./dist/native/bool/src`: a
/// sibling test had just deleted the working directory out from under the
/// build). Keying it by the process id closed that for nextest and left it open
/// for plain `cargo test`, which runs the tests as THREADS of one process: 38 of
/// 46 failed there, each deleting the others' staging (N129). The process id
/// still leads the name, so two concurrent runs of this binary cannot meet
/// either. The shared cargo target directory stays shared on purpose; cargo
/// locks it itself.
///
/// Nothing removed the directory at the test's end, so a run that started
/// clean left every staged corpus copy behind: 2,788 of them after one day of
/// Order 42's runs, and CI's `vilan-fmt` leg — walking `./target` locally —
/// tripped on 5,556 stale `resource struct` copies from before B413 (N133).
/// `StagedDir` removes its directory on drop, at the end of the test function
/// that called `stage()`, whether the test passed, failed, or panicked.
struct StagedDir(PathBuf);

impl std::ops::Deref for StagedDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for StagedDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for StagedDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn stage() -> StagedDir {
    static STAGED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let call = STAGED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let staged = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "native-differential-src-{}-{call}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&staged);
    copy_tree(&corpus_dir(), &staged);
    StagedDir(staged)
}

/// Copies the corpus tree, DIRECTORIES INCLUDED.
///
/// The files-only copy was a harness defect that read as a backend one:
/// `module-dirs.vl` imports `pkg::nested::…`, whose modules live in
/// `vilan/test/nested/`, and a staging directory without it failed the native
/// leg with `cannot find 'nested' in the imported path` — which `compare`
/// classifies as BROKEN, because the message carries no refusal. The JS leg was
/// never reached, so the program looked like an emitter failure while the JS
/// backend would have failed identically. One recursive copy, and the
/// `dist/native/` a build writes still lands in the copy rather than the tree.
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create the staging directory");
    for entry in std::fs::read_dir(from)
        .expect("read the corpus directory")
        .flatten()
    {
        let path = entry.path();
        let name = path.file_name().expect("a corpus entry has a name");
        // `dist` is a build artifact of the tree, not a corpus input.
        if name == "dist" || name == "target" {
            continue;
        }
        if path.is_dir() {
            copy_tree(&path, &to.join(name));
        } else if path.is_file() {
            std::fs::copy(&path, to.join(name)).expect("stage a corpus program");
        }
    }
}

fn vilan(staged: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command
        .current_dir(staged)
        .env("VILAN_STD", std_dir())
        .env("VILAN_RT", runtime_dir())
        .env("CARGO_TARGET_DIR", shared_target());
    command
}

/// What one program did.
#[derive(Debug, PartialEq)]
enum Verdict {
    /// The emitter named a construct it does not reach. The reason, verbatim.
    Refused(String),
    Identical,
    /// Accepted and wrong: the two backends printed different things, or the
    /// emitted Rust did not build. The detail is the report.
    Broken(String),
}

fn compare(staged: &Path, program: &str) -> Verdict {
    let native = vilan(staged)
        .args(["run", "--backend", "rust", program])
        .output()
        .expect("run the native backend");
    if !native.status.success() {
        let message = String::from_utf8_lossy(&native.stderr).into_owned();
        if message.contains("does not emit") {
            let reason = message
                .lines()
                .find(|line| line.contains("does not emit"))
                .unwrap_or("")
                .trim()
                .to_string();
            return Verdict::Refused(reason);
        }
        return Verdict::Broken(format!("the native leg failed:\n{message}"));
    }
    let javascript = vilan(staged)
        .args(["run", program])
        .output()
        .expect("run the JS backend");
    if !javascript.status.success() {
        return Verdict::Broken(format!(
            "the JS leg failed, so there is nothing to compare against:\n{}",
            String::from_utf8_lossy(&javascript.stderr)
        ));
    }
    if native.stdout == javascript.stdout {
        return Verdict::Identical;
    }
    Verdict::Broken(format!(
        "stdout differs.\n  js:   {:?}\n  rust: {:?}",
        String::from_utf8_lossy(&javascript.stdout),
        String::from_utf8_lossy(&native.stdout),
    ))
}

#[test]
fn the_default_suite_is_byte_identical_on_both_backends() {
    let staged = stage();
    let mut broken = Vec::new();
    for program in DEFAULT_SUITE {
        match compare(&staged, program) {
            Verdict::Identical => {}
            Verdict::Refused(reason) => broken.push(format!(
                "{program}: the default suite must be programs the backend ACCEPTS, and this one \
                 is refused — {reason}"
            )),
            Verdict::Broken(detail) => broken.push(format!("{program}: {detail}")),
        }
    }
    assert!(
        broken.is_empty(),
        "the native backend disagrees with the JS backend:\n{}",
        broken.join("\n")
    );
}

/// I5 ruling 2: the one program [`OUTSIDE_THE_DIFFERENTIAL`] names does what
/// the book says on each backend — JS prints the negative value and exits 0, a
/// native debug build panics on the subtraction — and the enumeration leaves it
/// out, so the sweep cannot call it broken.
#[test]
fn an_underflowing_usize_is_outside_the_differential_by_name() {
    for (program, _) in OUTSIDE_THE_DIFFERENTIAL {
        assert!(
            corpus_dir().join(program).is_file(),
            "{program} is named outside the differential but is not a corpus program"
        );
        assert!(
            !platform_free_programs().contains(&program.to_string()),
            "{program} is named outside the differential but the sweep still enumerates it"
        );
        assert!(
            !DEFAULT_SUITE.contains(program),
            "{program} is outside the differential and cannot be in its default suite"
        );
    }
    let staged = stage();
    let javascript = vilan(&staged)
        .args(["run", "usize-underflow.vl"])
        .output()
        .expect("run the JS backend");
    assert!(
        javascript.status.success(),
        "the JS leg runs through the underflow: {}",
        String::from_utf8_lossy(&javascript.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&javascript.stdout),
        "-1\ntrue\ntrue\n0\n"
    );
    let native = vilan(&staged)
        .args(["run", "--backend", "rust", "usize-underflow.vl"])
        .output()
        .expect("run the native backend");
    let stderr = String::from_utf8_lossy(&native.stderr);
    assert!(
        !native.status.success() && stderr.contains("attempt to subtract with overflow"),
        "the native debug build panics on the subtraction, it does not print a value:\n\
         stdout: {}\nstderr: {stderr}",
        String::from_utf8_lossy(&native.stdout)
    );
}

/// N106: the other [`OUTSIDE_THE_DIFFERENTIAL`] program — negative zero is
/// the one `vilan_rt::js_number` departure from `console.log` that is NOT
/// identical: node's `console.log` special-cases it to `"-0"` (`util.inspect`),
/// while every JS *stringification* (`String(x)`, a template literal,
/// `JSON.stringify`) answers `"0"`, which is what `js_number` — and so the
/// native backend's `print` — implements. Both backends do what the book
/// says; the sweep leaves the program out so it is not called broken.
#[test]
fn negative_zero_prints_differently_on_each_backend_by_name() {
    for (program, _) in OUTSIDE_THE_DIFFERENTIAL {
        assert!(
            corpus_dir().join(program).is_file(),
            "{program} is named outside the differential but is not a corpus program"
        );
        assert!(
            !platform_free_programs().contains(&program.to_string()),
            "{program} is named outside the differential but the sweep still enumerates it"
        );
        assert!(
            !DEFAULT_SUITE.contains(program),
            "{program} is outside the differential and cannot be in its default suite"
        );
    }
    let staged = stage();
    let javascript = vilan(&staged)
        .args(["run", "f64-print-negative-zero.vl"])
        .output()
        .expect("run the JS backend");
    assert!(
        javascript.status.success(),
        "the JS leg runs clean: {}",
        String::from_utf8_lossy(&javascript.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&javascript.stdout), "-0\n");
    let native = vilan(&staged)
        .args(["run", "--backend", "rust", "f64-print-negative-zero.vl"])
        .output()
        .expect("run the native backend");
    assert!(
        native.status.success(),
        "the native leg runs clean: {}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&native.stdout), "0\n");
}

/// The whole platform-free corpus, under `VILAN_NATIVE_DIFFERENTIAL=1`.
///
/// It prints the census — refused / identical — because that census IS the
/// slice's number, and a reader who runs this wants to see it move.
#[test]
fn every_platform_free_program_is_identical_or_named() {
    if std::env::var_os("VILAN_NATIVE_DIFFERENTIAL").is_none() {
        eprintln!(
            "skipped: set VILAN_NATIVE_DIFFERENTIAL=1 to sweep the whole corpus \
             (rustc is ~1 s per program)"
        );
        return;
    }
    let staged = stage();
    let programs = platform_free_programs();
    let mut identical_programs: Vec<String> = Vec::new();
    let mut refused: Vec<(String, String)> = Vec::new();
    let mut broken = Vec::new();
    for program in &programs {
        match compare(&staged, program) {
            Verdict::Identical => identical_programs.push(program.clone()),
            Verdict::Refused(reason) => refused.push((program.clone(), reason)),
            Verdict::Broken(detail) => broken.push(format!("{program}: {detail}")),
        }
    }
    eprintln!(
        "native differential: {} enumerated, {} identical, {} refused by name, {} broken",
        programs.len(),
        identical_programs.len(),
        refused.len(),
        broken.len()
    );
    for (program, reason) in &refused {
        eprintln!("  refused  {program}: {reason}");
    }
    for program in &identical_programs {
        eprintln!("  identical  {program}");
    }
    assert!(
        broken.is_empty(),
        "programs the native backend ACCEPTED and then got wrong:\n{}",
        broken.join("\n")
    );
}

/// `native-apps.md`'s own probe — **the differential this pin was written to
/// become** (F20).
///
/// It was a named-gap pin for two slices. S1a's wall was `SignalCell<i32>`, a
/// generic type; S1b removed every wall it named and recorded the host type
/// `Hash`. F20 built `Hash` (`vilan_rt::Hash`, the `CanonicalHash`/`HashEq`
/// intrinsics and `JSON.stringify`'s own renderer), and behind it stood
/// `std::reactive`'s three glue bindings (`__guarded`, `__with_finally`,
/// `queueMicrotask`) and five move/copy defects the program was the first to
/// reach: `Shared::write()` as a place, a `Shared` binding read twice, a `move`
/// closure taking a capture the frame still needs, a `match` leg destructuring a
/// place the body reads afterwards, and a boxed binding pushed through a COPY of
/// its value (which printed `0 0` against the JS backend's `2 2`).
///
/// So the assertion is now the claim rather than the gap: the paper's probe
/// prints the same bytes as a native binary and as a JS one.
#[test]
fn the_probes_board_program_is_byte_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_board.vl"), BOARD_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_board.vl"),
        Verdict::Identical,
        "`native-apps.md`'s board probe is the native path's exit; it must print the same bytes \
         on both backends"
    );
}

/// The four lowering classes a `std::http` server program reaches and no corpus
/// program does (F20, found by lane native-b-39 while building the HTTP
/// runtime).
///
/// They are pinned as ONE probe rather than four because they are one shape —
/// an aggregate that holds a callback — seen from four sides, and because the
/// program that found them is not in the corpus, so the whole-set gate cannot
/// see any of them. In order: a struct field declared `async |T| U` (the type is
/// `Rc<dyn Fn(T) -> Boxed<U>>` and a SYNC literal landing in it is wrapped,
/// which is `Server::builder()`'s default handler); a field holding an `Option`
/// of a closure, which defeats the derived `PartialEq` exactly as a bare closure
/// field does; an ENUM payload holding a closure, which `ensure_enum` guarded
/// neither for equality nor for rendering; and a non-`Copy` field read off a
/// `&`-loaned receiver, which is a MOVE out of a shared reference where
/// `clone_sites` elided rule 1's copy.
#[test]
fn an_aggregate_holding_a_callback_compiles_the_same_way_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_callbacks.vl"), CALLBACK_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_callbacks.vl"),
        Verdict::Identical,
        "an aggregate holding a callback must compile and print identically on both backends"
    );
}

const CALLBACK_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    // A struct field declared `async |T| U`, plus an `Option` of a closure.
    "struct Server {\n",
    "\thandler: async |i32| str,\n",
    "\ton_close: Option<|| void>,\n",
    "\tname: str,\n",
    "}\n",
    "\n",
    "impl Server {\n",
    "\tfun builder(): Server {\n",
    // A SYNC literal in the async field — the wrap.
    "\t\tServer { handler = |n| { \"default\" }, on_close = None, name = \"s\" }\n",
    "\t}\n",
    "\n",
    "\tasync fun serve(self, n: i32): str {\n",
    "\t\tawait (self.handler)(n)\n",
    "\t}\n",
    "}\n",
    "\n",
    // An enum PAYLOAD holding a closure.
    "enum Body {\n",
    "\tFixed(str),\n",
    "\tStream(|| str),\n",
    "}\n",
    "\n",
    "fun render(body: Body): str {\n",
    "\tmatch body {\n",
    "\t\tBody::Fixed(let text) => text,\n",
    "\t\tBody::Stream(let make) => make(),\n",
    "\t}\n",
    "}\n",
    "\n",
    // A non-`Copy` field read off a loaned receiver.
    "struct Builder { body: str, items: List<i32> }\n",
    "\n",
    "impl Builder {\n",
    "\tfun build(self): Builder {\n",
    "\t\tBuilder { body = self.body, items = self.items }\n",
    "\t}\n",
    "}\n",
    "\n",
    "async fun main() {\n",
    "\tlet server = Server::builder();\n",
    "\tprint(await server.serve(1));\n",
    "\tlet custom = Server { handler = |n| { i\"n={n}\" }, on_close = None, name = \"c\" };\n",
    "\tprint(await custom.serve(7));\n",
    "\tprint(render(Body::Fixed(\"fixed\")));\n",
    "\tprint(render(Body::Stream(|| { \"streamed\" })));\n",
    "\tlet builder = Builder { body = \"b\", items = [1, 2] };\n",
    "\tlet copy = builder.build();\n",
    "\tprint(copy.body);\n",
    "\tprint(copy.items.len());\n",
    "}\n",
);

/// A `lazy` parameter defers, memoizes and FORWARDS the same way on both
/// backends (F20; `lazy.md` §1).
///
/// The corpus covers the feature well — thirteen programs reach a `lazy`
/// parameter, which is the whole `Option`/`Result` combinator surface since
/// A103 — but every one of them reaches it through a combinator whose argument
/// is a plain value, so none of them can tell a deferral from an eager
/// evaluation. This probe can: the argument counts its own evaluations and the
/// program prints the counter, so an emitter that forced at the call site prints
/// different numbers rather than the same ones.
#[test]
fn a_lazy_parameter_defers_and_memoizes_the_same_way_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_lazy.vl"), LAZY_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_lazy.vl"),
        Verdict::Identical,
        "a `lazy` parameter must defer, memoize and forward identically on both backends"
    );
}

const LAZY_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "mut evaluations = 0;\n",
    "\n",
    "fun costly(): str {\n",
    "\tevaluations = evaluations + 1;\n",
    "\ti\"built {evaluations}\"\n",
    "}\n",
    "\n",
    // The argument is never read, so the thunk never runs.
    "fun ignore(lazy message: str): i32 { 1 }\n",
    // Read TWICE: one evaluation, two reads.
    "fun twice(lazy message: str): str {\n",
    "\tlet first = message;\n",
    "\tlet second = message;\n",
    "\ti\"{first}/{second}\"\n",
    "}\n",
    // A FORWARD: the cell travels as-is, so the chain memoizes once.
    "fun forwards(lazy message: str): str { twice(message) }\n",
    "\n",
    "fun main() {\n",
    "\tprint(ignore(costly()));\n",
    "\tprint(evaluations);\n",
    "\tprint(twice(costly()));\n",
    "\tprint(evaluations);\n",
    "\tprint(forwards(costly()));\n",
    "\tprint(evaluations);\n",
    // A literal in the same position still works, and so does a value the
    // callee reads on only one of two paths.
    "\tprint(twice(\"plain\"));\n",
    "\tprint(evaluations);\n",
    // A103's own customers: `unwrap_or`'s fallback is `lazy`, so the `Some`
    // path never builds it and the `None` path does.
    "\tlet present: Option<usize> = Some(3);\n",
    "\tprint(present.unwrap_or(costly().len()));\n",
    "\tprint(evaluations);\n",
    "\tlet absent: Option<usize> = None;\n",
    "\tprint(absent.unwrap_or(costly().len()));\n",
    "\tprint(evaluations);\n",
    "}\n",
);

/// The canonical key, as the JS backend's `__hash` computes it (F20): a
/// primitive keys as ITSELF and an aggregate keys as its `JSON.stringify` text,
/// so the four `Hash` arms are the four JS primitive kinds and nothing else.
///
/// Every stock `impl Hashable` in `std::hash` is exercised through a `Map` key,
/// because keying is the only thing a `Hash` is for and a wrong canonicalisation
/// shows up as a lookup that misses or a duplicate that collides — not as a
/// printed value. The `1` / `"1"` pair is the case a naive
/// canonicalise-to-a-string would get wrong: two DIFFERENT JS primitives, so two
/// different keys.
#[test]
fn a_canonical_hash_keys_the_same_values_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_hash.vl"), HASH_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_hash.vl"),
        Verdict::Identical,
        "a canonical hash must key the same values on both backends"
    );
}

const HASH_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::hash_map::HashMap;\n",
    "import std::hash_set::HashSet;\n",
    "\n",
    "[derive(Hashable, PartialEq)]\n",
    "struct Point { x: i32, y: i32 }\n",
    "\n",
    "fun main() {\n",
    // A string key and an integer key that render the same: two JS primitives,
    // two keys.
    "\tmut mixed: HashMap<str, i32> = HashMap::new();\n",
    "\tmixed.insert(\"1\", 10);\n",
    "\tmixed.insert(\"one\", 11);\n",
    "\tprint(mixed.len());\n",
    "\tprint(mixed.get(\"1\"));\n",
    "\tmut numbers: HashMap<i32, str> = HashMap::new();\n",
    "\tnumbers.insert(1, \"one\");\n",
    "\tnumbers.insert(2, \"two\");\n",
    "\tnumbers.insert(1, \"uno\");\n",
    "\tprint(numbers.len());\n",
    "\tprint(numbers.get(1));\n",
    // A re-insert keeps the ORIGINAL position, as a JS `Map` does.
    "\tfor key in numbers.keys() { print(key); }\n",
    // `bool` and `f64` keys — the other two primitive arms.
    "\tmut flags: HashMap<bool, i32> = HashMap::new();\n",
    "\tflags.insert(true, 1);\n",
    "\tflags.insert(false, 0);\n",
    "\tprint(flags.get(true));\n",
    "\tprint(flags.contains_key(false));\n",
    "\tmut reals: HashMap<f64, str> = HashMap::new();\n",
    "\treals.insert(1.5, \"half\");\n",
    "\treals.insert(0.0, \"zero\");\n",
    "\tprint(reals.get(1.5));\n",
    "\tprint(reals.len());\n",
    // An AGGREGATE key: `[derive(Hashable)]` canonicalises through
    // `JSON.stringify`, so two equal points are one key and a different one is
    // its own.
    "\tmut points: HashMap<Point, str> = HashMap::new();\n",
    "\tpoints.insert(Point { x = 1, y = 2 }, \"a\");\n",
    "\tpoints.insert(Point { x = 1, y = 2 }, \"b\");\n",
    "\tpoints.insert(Point { x = 2, y = 1 }, \"c\");\n",
    "\tprint(points.len());\n",
    "\tprint(points.get(Point { x = 1, y = 2 }));\n",
    // A `List` key — `impl List<T: Hashable> with Hashable`.
    "\tmut lists: HashMap<List<i32>, str> = HashMap::new();\n",
    "\tlists.insert([1, 2], \"twelve\");\n",
    "\tprint(lists.get([1, 2]));\n",
    "\tprint(lists.get([2, 1]));\n",
    // A `Set`, which keys the same way.
    "\tmut words: HashSet<str> = HashSet::new();\n",
    "\twords.insert(\"a\");\n",
    "\twords.insert(\"a\");\n",
    "\twords.insert(\"b\");\n",
    "\tprint(words.len());\n",
    "\tprint(words.contains(\"b\"));\n",
    "\tprint(words.contains(\"z\"));\n",
    "}\n",
);

/// A138 S0 (`reactive-maps-sets.md` §6.1, I10): insertion order is the contract
/// of `HashMap` and `HashSet`, and the case the overwrite pin above does not
/// reach is a key REMOVED and inserted again — it goes to the END, on both
/// backends, as a JS `Map` puts it. The native map keeps a tombstone where the
/// key stood, so a runtime that revived the tombstone (or a compaction that
/// re-slotted it) would print the key back in its old place; the walks below
/// print the order after each step, with a removal at the front, in the middle
/// and of the last key, then a churn through the same key that leaves a hole
/// behind every pass.
#[test]
fn a_removed_and_reinserted_key_goes_to_the_end_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_reinsert.vl"),
        REINSERT_ORDER_PROBE,
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_reinsert.vl"),
        Verdict::Identical,
        "a removed and re-inserted key must go to the end on both backends"
    );
}

const REINSERT_ORDER_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::hash_map::HashMap;\n",
    "import std::hash_set::HashSet;\n",
    "\n",
    "fun main() {\n",
    "\tmut map: HashMap<str, i32> = HashMap::new();\n",
    "\tmap.insert(\"a\", 1);\n",
    "\tmap.insert(\"b\", 2);\n",
    "\tmap.insert(\"c\", 3);\n",
    // The front leaves and comes back: the end.
    "\tmap.remove(\"a\");\n",
    "\tmap.insert(\"a\", 10);\n",
    "\tprint(map.keys());\n",
    // The middle.
    "\tmap.remove(\"c\");\n",
    "\tmap.insert(\"d\", 4);\n",
    "\tmap.insert(\"c\", 30);\n",
    "\tprint(map.keys());\n",
    "\tprint(map.values());\n",
    // The last key, out and back: nothing moves.
    "\tmap.remove(\"c\");\n",
    "\tmap.insert(\"c\", 31);\n",
    "\tprint(map.entries());\n",
    // A churn through one key, with another arriving between passes.
    "\tmut pass = 0;\n",
    "\tfor pass < 3 {\n",
    "\t\tmap.remove(\"b\");\n",
    "\t\tmap.insert(\"e\", pass);\n",
    "\t\tmap.remove(\"e\");\n",
    "\t\tmap.insert(\"b\", pass);\n",
    "\t\tpass += 1;\n",
    "\t}\n",
    "\tprint(map.keys());\n",
    "\tprint(map.len());\n",
    // The set's members, the same way.
    "\tmut set: HashSet<i32> = [3, 1, 2].to_set();\n",
    "\tset.remove(3);\n",
    "\tset.insert(3);\n",
    "\tset.insert(1);\n",
    "\tprint(set.values());\n",
    "\tset.remove(2);\n",
    "\tset.insert(4);\n",
    "\tset.insert(2);\n",
    "\tprint(set.values());\n",
    "}\n",
);

/// I9 / Q8: `HashMap` and `HashSet` equality is ORDER-INSENSITIVE on both
/// backends — the same keys with equal values (the same members), whatever the
/// insertion order. The native runtime's own `PartialEq for Map` is
/// order-SENSITIVE and must stay unreachable: `==` goes through std's
/// `impl .. with PartialEq`, and so does a derived `PartialEq` over a struct
/// holding a map, and a map of maps compares its values through the same impl.
/// Each line pairs an order-only difference (equal) with a value, a size or a
/// member difference (unequal).
#[test]
fn i9_hash_collection_equality_is_order_insensitive_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_hash_eq.vl"), HASH_EQUALITY_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_hash_eq.vl"),
        Verdict::Identical,
        "map and set equality must ignore insertion order on both backends"
    );
}

const HASH_EQUALITY_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::hash_map::HashMap;\n",
    "import std::hash_set::HashSet;\n",
    "\n",
    "[derive(PartialEq)]\n",
    "struct Holder {\n",
    "\tscores: HashMap<str, i32>,\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tmut a: HashMap<str, i32> = HashMap::new();\n",
    "\ta.insert(\"x\", 1);\n",
    "\ta.insert(\"y\", 2);\n",
    "\tmut b: HashMap<str, i32> = HashMap::new();\n",
    "\tb.insert(\"y\", 2);\n",
    "\tb.insert(\"x\", 1);\n",
    "\tprint(a == b);\n",
    "\tb.insert(\"y\", 3);\n",
    "\tprint(a == b);\n",
    "\tprint(a != b);\n",
    "\tmut c: HashMap<str, i32> = HashMap::new();\n",
    "\tc.insert(\"x\", 1);\n",
    "\tprint(a == c);\n",
    "\tc.insert(\"z\", 2);\n",
    "\tprint(a == c);\n",
    "\tprint(Holder { scores = a } == Holder { scores = [(\"y\", 2), (\"x\", 1)].to_map() });\n",
    "\tprint(Holder { scores = a } == Holder { scores = c });\n",
    "\tlet s: HashSet<i32> = [1, 2, 3].to_set();\n",
    "\tprint(s == [3, 2, 1].to_set());\n",
    "\tprint(s == [3, 2].to_set());\n",
    "\tprint(s == [3, 2, 4].to_set());\n",
    "\tmut nested: HashMap<str, HashMap<str, i32>> = HashMap::new();\n",
    "\tnested.insert(\"a\", a);\n",
    "\tmut other: HashMap<str, HashMap<str, i32>> = HashMap::new();\n",
    "\tother.insert(\"a\", [(\"y\", 2), (\"x\", 1)].to_map());\n",
    "\tprint(nested == other);\n",
    "}\n",
);

/// A142 S4 and S5: the collection pipes on both backends — the operators' and
/// the boundary conversions' seeded random walks against an oracle recomputed
/// after every step (the same walks as `inference/collections.rs`'s), and the
/// following shapes: `map` over a
/// returned pipe and a returned source, `flatten`, `filter_map(|m| m)` and
/// `any(|m| m.is_pending())` over `Option`-valued sources, and the element
/// runs' owners released on leave, re-run and disposal.
#[test]
fn a142_collection_pipes_build_the_same_on_both_backends() {
    let staged = stage();
    for (name, program) in [
        (
            "native_probe_collection_walk.vl",
            include_str!("native/collection_walk.vl"),
        ),
        (
            "native_probe_collection_follow.vl",
            include_str!("native/collection_follow.vl"),
        ),
        (
            "native_probe_conversion_walk.vl",
            include_str!("native/conversion_walk.vl"),
        ),
    ] {
        std::fs::write(staged.join(name), program).expect("write the probe program");
        assert_eq!(
            compare(&staged, name),
            Verdict::Identical,
            "{name}: a collection pipe must build and answer the same on both backends"
        );
    }
}

/// A138 S1 and S2: `HashMapCell`, `HashSetCell` and the map operators on both backends —
/// their seeded random walks (`inference/maps.rs` runs the same programs on JS),
/// checked against a plain `HashMap`/`HashSet` after every write: the collection
/// and its order, a mirror replayed from the drained ops, every watched key's
/// handle and how often it woke — plus `keys()`, a set pipe, and a `HashMapEntry`'s
/// `set(Some(v))`/`set(None)` writes. `HashSetCell`, `SetOp` and `keys()` were
/// refused natively until F72 (a variant of the one-parameter, BOUNDED
/// `SetOp<T: Hashable>` was typed as a bare trait object).
#[test]
fn a138_map_and_set_cells_build_the_same_on_both_backends() {
    let staged = stage();
    for (name, program, what) in [
        (
            "native_probe_map_walk.vl",
            include_str!("native/map_walk.vl"),
            "a HashMapCell",
        ),
        // A138 S2: the map operators' walk.
        (
            "native_probe_map_operator_walk.vl",
            include_str!("native/map_operator_walk.vl"),
            "the map operators",
        ),
        (
            "native_probe_set_walk.vl",
            include_str!("native/set_walk.vl"),
            "a HashSetCell",
        ),
        (
            "native_probe_map_keys_pipe.vl",
            include_str!("native/map_keys_pipe.vl"),
            "`keys()` and `values()`",
        ),
        (
            "native_probe_map_entry_writes.vl",
            include_str!("native/map_entry_writes.vl"),
            "a HashMapEntry's writes",
        ),
    ] {
        std::fs::write(staged.join(name), program).expect("write the probe program");
        assert_eq!(
            compare(&staged, name),
            Verdict::Identical,
            "{what} must build and answer the same on both backends"
        );
    }
}

/// F23: a context-threaded hidden parameter is typed from the flavour the
/// CONTEXT PASS recorded, and one program carries both readings.
///
/// The parameter is deliberately record-less (no `parameters` entry, no span, no
/// type — editing-dx.md §19.3), so the Rust emitter, which must write a type
/// down, used to re-derive the flavour from the arguments every call site passes
/// — a worklist that had to connect clause-typed parameters to every closure
/// literal that can land there, and that fell back on the strict reading when
/// nothing settled. `context.rs` knows the answer where it MINTS the parameter:
/// a node holds the bare value when its provider is strict or a `run` closure,
/// and `Option<T>` otherwise.
///
/// The probe puts a safe reader (`peek`, which `get_safe`s) and a strict one
/// (`strict_report`, which `get`s and then calls `peek`) in one program, so the
/// two flavours are asserted against each other rather than one at a time: a
/// record that is uniformly wrong, or uniformly right for the wrong reason,
/// cannot pass both halves. Flipping the recorded bool swaps the two signatures,
/// which is what makes this non-vacuous.
#[test]
fn a_context_threaded_parameter_is_typed_from_the_recorded_flavour() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_context_flavour.vl"),
        CONTEXT_PROBE,
    )
    .expect("write the probe program");
    let emitted = vilan(&staged)
        .args([
            "build",
            "--backend",
            "rust",
            "--stdout",
            "native_probe_context_flavour.vl",
        ])
        .output()
        .expect("build the probe");
    assert!(
        emitted.status.success(),
        "the context-flavour probe was refused:\n{}",
        String::from_utf8_lossy(&emitted.stderr)
    );
    let source = String::from_utf8_lossy(&emitted.stdout);
    let signature = |prefix: &str| -> String {
        source
            .lines()
            .find(|line| line.starts_with(&format!("fn {prefix}")))
            .unwrap_or_else(|| panic!("no `fn {prefix}..` in the emitted source:\n{source}"))
            .to_string()
    };
    // The safe region's parameter carries the `Option`; the strict region's
    // carries the value, because `run` hands it one.
    let safe = signature("peek_");
    assert!(
        safe.contains(": Option<i32>"),
        "a `get_safe`-reachable region's hidden parameter must be `Option<i32>`: {safe}"
    );
    let strict = signature("strict_report_");
    assert!(
        strict.contains(": i32") && !strict.contains(": Option<i32>"),
        "a strict region's hidden parameter must be the bare value: {strict}"
    );
    // And the program means the same thing on both backends, which is what the
    // types have to be right FOR.
    assert_eq!(
        compare(&staged, "native_probe_context_flavour.vl"),
        Verdict::Identical,
        "the context-threaded program must agree on both backends"
    );
}

/// Both context flavours in one program: `peek` is safe (it `get_safe`s, so its
/// hidden parameter is an `Option`), `strict_report` is strict (it `get`s, so
/// `run` hands it the bare value) and calls `peek`, which is the covered→safe
/// boundary that `Some`-wraps.
const CONTEXT_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::context::Context;\n",
    "import std::option::Option::{ Some, None };\n",
    "\n",
    "let current: Context<i32> = Context::new();\n",
    "\n",
    "fun peek(): str {\n",
    "\tmatch current.get_safe() {\n",
    "\t\tSome(let value) => i\"peeked {value}\",\n",
    "\t\tNone => \"nothing\",\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun strict_report() {\n",
    "\tlet value = current.get();\n",
    "\tprint(i\"strict {value}\");\n",
    "\tprint(peek());\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tprint(peek());\n",
    "\tcurrent.run(9, || {\n",
    "\t\tstrict_report();\n",
    "\t});\n",
    "\tprint(peek());\n",
    "}\n",
);

/// A destructuring `let` means the same thing on both backends (F18).
///
/// `std::http`'s response loop is what wanted it — `for header in
/// response.headers { let (name, value) = header; .. }` — and the emitter
/// refused the form by name. It is not an HTTP construct, so it is pinned as
/// what it is: a tuple pattern in a `let`, nested, with a wildcard, over a
/// binding and over a loop binder.
#[test]
fn a_destructuring_let_is_byte_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_destructure.vl"),
        DESTRUCTURE_PROBE,
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_destructure.vl"),
        Verdict::Identical,
        "a destructuring `let` must mean the same thing on both backends"
    );
}

const DESTRUCTURE_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun main() {\n",
    "\tlet pair = (1, \"one\");\n",
    "\tlet (number, word) = pair;\n",
    "\tprint(number);\n",
    "\tprint(word);\n",
    "\tlet nested = ((2, 3), \"two\");\n",
    "\tlet ((left, right), label) = nested;\n",
    "\tprint(left + right);\n",
    "\tprint(label);\n",
    "\tlet (_, kept) = pair;\n",
    "\tprint(kept);\n",
    "\tlet rows = [(1, \"a\"), (2, \"b\")];\n",
    "\tfor row in rows {\n",
    "\t\tlet (index, name) = row;\n",
    "\t\tprint(i\"{index}={name}\");\n",
    "\t}\n",
    "}\n",
);

/// F41: a field named `self`, `super` or `crate` builds natively.
///
/// Vilan's `self` and `super` are contextual, so they are legal field names
/// and the JS backend always ran them; the emitter spelled them `r#self` /
/// `r#super`, and rustc refuses a PATH keyword raw. They are mangled instead
/// (`self` → `self_`), and a field ALREADY spelled `self_` sits beside
/// them in the probe so the mangling is shown injective: one more `_` for
/// every name in the family, never a collision. Every site the name reaches is
/// here — the declaration, a literal, a read, a write, a compound write, a
/// method body through `self.self`, the derived `PartialEq` and `Json`, the
/// printed struct, and a closure-holding struct's hand-written `PartialEq`.
#[test]
fn a_field_named_by_a_path_keyword_builds_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_path_keywords.vl"),
        PATH_KEYWORD_FIELD_PROBE,
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_path_keywords.vl"),
        Verdict::Identical,
        "a field named `self`/`super`/`crate` must build and print identically on both backends"
    );
}

const PATH_KEYWORD_FIELD_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "[derive(Json, PartialEq)]\n",
    "struct Node {\n",
    "\tself: i32,\n",
    "\tsuper: str,\n",
    "\tself_: i32,\n",
    "\tcrate: bool,\n",
    "}\n",
    "\n",
    "struct Hook {\n",
    "\tself: || i32,\n",
    "\tsuper: i32,\n",
    "}\n",
    "\n",
    "impl Node {\n",
    "\tfun total(self): i32 {\n",
    "\t\tself.self + self.self_\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tmut node = Node { self = 1, super = \"up\", self_ = 10, crate = true };\n",
    "\tnode.self = node.self + 1;\n",
    "\tnode.self_ += node.self;\n",
    "\tprint(i\"{node.self} {node.super} {node.self_} {node.crate} {node.total()}\");\n",
    "\tprint(node);\n",
    "\tprint(node.to_json());\n",
    "\tlet hook = Hook { self = || 7, super = 8 };\n",
    "\tlet call = hook.self;\n",
    "\tprint(call() + hook.super);\n",
    "\tlet copy = Node { self = 5, super = \"s\", self_ = 6, crate = false };\n",
    "\tprint(copy == node);\n",
    "\tprint(copy.self == 5 && copy.super == \"s\");\n",
    "}\n",
);

/// F42: `KeyedCell` builds natively — every writer, the op log read back as
/// the wire's `Delta`s, the `Source` view, the keyed lookup and the wholesale
/// `set`.
///
/// It was refused whole, for `Map`'s unbound `V`, at `KeyedCell::new`'s
/// `positions = Shared::new(Map::new())`. The field's type names both of
/// `Map`'s arguments, and a generic call closes its open bindings from the
/// position it fills — but `Shared::new` is an INTRINSIC, and its argument
/// was rendered under the expectation for the intrinsic's RESULT: `Map<K, V>`
/// matched against `Shared<Map<Hash, usize>>` closed nothing. Each argument
/// of an intrinsic now takes the expectation its intrinsic gives it — the
/// `Shared`'s element for `Shared::new`'s value, nothing for a receiver or
/// an operand, which the result says nothing about.
#[test]
fn a_keyed_cell_builds_and_journals_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_keyed_cell.vl"), KEYED_CELL_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_keyed_cell.vl"),
        Verdict::Identical,
        "`KeyedCell` must build natively and journal the same deltas as node"
    );
}

const KEYED_CELL_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::reactive::Source;\n",
    "import std::rpc::KeyedCell;\n",
    "import std::wire::{ Delta, Keyed };\n",
    "\n",
    "struct Row {\n",
    "\tid: i32,\n",
    "\tlabel: str,\n",
    "}\n",
    "\n",
    "impl Row with Keyed<i32> {\n",
    "\tfun key(self): i32 {\n",
    "\t\tself.id\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun describe(delta: Delta<i32, Row>): str {\n",
    "\tmatch delta {\n",
    "\t\tDelta::Reset(let rows) => i\"reset {rows.len()}\",\n",
    "\t\tDelta::Insert(let key, let row, let at) => i\"insert {key} {row.label} at {at}\",\n",
    "\t\tDelta::Update(let key, let row) => i\"update {key} {row.label}\",\n",
    "\t\tDelta::Remove(let key) => i\"remove {key}\",\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun labels(rows: List<Row>): str {\n",
    "\tmut joined = \"\";\n",
    "\tfor row in rows {\n",
    "\t\tjoined = joined + row.label + \";\";\n",
    "\t}\n",
    "\tjoined\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet cell: KeyedCell<i32, Row> = KeyedCell<i32, Row>::new([Row { id = 1, label = \"one\" }]);\n",
    "\tlet cursor = cell.cursor();\n",
    "\tcell.insert(Row { id = 2, label = \"two\" });\n",
    "\tcell.insert(Row { id = 3, label = \"three\" });\n",
    "\tcell.update(2, |&mut row| {\n",
    "\t\trow.label = \"TWO\";\n",
    "\t});\n",
    "\tcell.remove(1);\n",
    "\tcell.insert(Row { id = 3, label = \"THREE\" });\n",
    "\tfor delta in cell.since(cursor) {\n",
    "\t\tprint(describe(delta));\n",
    "\t}\n",
    "\tprint(labels(cell.get()));\n",
    "\tmatch cell.locate(3) {\n",
    "\t\tSome(let found) => print(i\"3 at {found.0}: {found.1.label}\"),\n",
    "\t\tNone => print(\"3 missing\"),\n",
    "\t}\n",
    "\tcell.set([Row { id = 9, label = \"nine\" }]);\n",
    "\tprint(labels(cell.get()));\n",
    "\tfor delta in cell.since(cursor) {\n",
    "\t\tprint(describe(delta));\n",
    "\t}\n",
    "}\n",
);

/// F44: a closure stored where nothing names its type is the counted
/// `dyn Fn` its vilan type renders as — including one pushed into the list it
/// READS.
///
/// `Shared::new([])` gives Rust's inference nothing, so the first closure
/// pushed decided the element type: its own anonymous one. A second closure
/// was then "a different closure", and a closure that reads the list it is
/// pushed into was "a cyclic type of infinite size" — the closure's type held
/// the list that held the closure. Every closure literal is now built AS
/// `Rc<dyn Fn(..) -> _>`, so the element type is the one the vilan type
/// names. The three shapes: a self-reading closure beside a second one in a
/// `Shared<List<..>>`, two closures grown into an empty `List`, and an
/// `Option` holding a closure that reads the cell holding the option.
#[test]
fn a_closure_stored_in_what_it_reads_builds_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_stored_closure.vl"),
        STORED_CLOSURE_PROBE,
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_stored_closure.vl"),
        Verdict::Identical,
        "a closure stored in a collection it reads must build and answer the same on both backends"
    );
}

const STORED_CLOSURE_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::shared::Shared;\n",
    "\n",
    "fun main() {\n",
    "\t// A closure pushed into the very list it reads, then a second closure.\n",
    "\tlet cell: Shared<List<|| usize>> = Shared::new([]);\n",
    "\tlet same = cell;\n",
    "\tcell.write().push(|| same.read().len());\n",
    "\tcell.write().push(|| 40);\n",
    "\tmut total: usize = 0;\n",
    "\tfor call in cell.read() {\n",
    "\t\ttotal += call();\n",
    "\t}\n",
    "\tprint(total);\n",
    "\n",
    "\t// A plain list of two different closures, built empty and grown.\n",
    "\tmut steps: List<|i32| i32> = [];\n",
    "\tsteps.push(|n| n + 1);\n",
    "\tsteps.push(|n| n * 10);\n",
    "\tmut value = 1;\n",
    "\tfor step in steps {\n",
    "\t\tvalue = step(value);\n",
    "\t}\n",
    "\tprint(value);\n",
    "\n",
    "\t// An `Option` holding a closure that reads the cell holding the option.\n",
    "\tlet hook: Shared<Option<|| str>> = Shared::new(None);\n",
    "\tlet seen = hook;\n",
    "\thook.write() = Some(|| if seen.read().is_some() { \"set\" } else { \"unset\" });\n",
    "\tmatch hook.read() {\n",
    "\t\tSome(let call) => print(call()),\n",
    "\t\tNone => print(\"none\"),\n",
    "\t}\n",
    "}\n",
);

/// F46: an argument that hands the RECEIVER's binding on by value copies it,
/// because the receiver's loan outlives every argument.
///
/// `source.on_settle(pulling(source, observer))` — `std::reactive`'s
/// `subscribe_pulling`, which reactive-42 wrote as two statements to dodge
/// this. The last-use pass walked the receiver first, so the argument's read
/// was the binding's last use and MOVED it while `&source` was live (rustc
/// E0505; E0382 behind F35's hoist for a `&mut` receiver). A loaned bare
/// place is now walked after the call's other arguments, so the argument
/// copies. Four shapes, each the binding's last use: a `&self` method, a
/// `&mut self` method (F35's hoist), a trait default, and the std shape
/// through a generic bound.
#[test]
fn an_argument_handing_on_the_borrowed_receiver_copies_it_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_loaned_receiver.vl"),
        LOANED_RECEIVER_PROBE,
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_loaned_receiver.vl"),
        Verdict::Identical,
        "an argument moving the borrowed receiver must build and print identically"
    );
}

const LOANED_RECEIVER_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "struct Counter {\n",
    "\thits: List<i32>,\n",
    "}\n",
    "\n",
    "impl Counter {\n",
    "\tfun absorb(self, other: Counter): i32 {\n",
    "\t\tself.hits.len().as_i32() + other.hits.len().as_i32()\n",
    "\t}\n",
    "\n",
    "\tfun grow(&mut self, other: Counter): i32 {\n",
    "\t\tself.hits.push(other.hits.len().as_i32());\n",
    "\t\tself.hits.len().as_i32()\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun twice(counter: Counter): Counter {\n",
    "\tmut hits = counter.hits;\n",
    "\thits.push(0);\n",
    "\tCounter { hits }\n",
    "}\n",
    "\n",
    "trait Source {\n",
    "\tfun size(self): i32;\n",
    "\tfun settle(self, witness: Counter): i32 {\n",
    "\t\tself.size() * 100 + witness.hits.len().as_i32()\n",
    "\t}\n",
    "}\n",
    "\n",
    "impl Counter with Source {\n",
    "\tfun size(self): i32 {\n",
    "\t\tself.hits.len().as_i32()\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun witness<S: Source>(source: S): Counter {\n",
    "\tCounter { hits = [source.size(), source.size()] }\n",
    "}\n",
    "\n",
    "// `std::reactive`'s `subscribe_pulling` shape: through a BOUND, the receiver\n",
    "// is borrowed while the argument hands the same parameter on by value.\n",
    "fun generic<S: Source>(source: S): i32 {\n",
    "\tsource.settle(witness(source))\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet counter = Counter { hits = [1, 2] };\n",
    "\t// The argument MOVES the receiver's binding at its last use, while the\n",
    "\t// receiver is borrowed for the call.\n",
    "\tprint(counter.absorb(twice(counter)));\n",
    "\tmut growing = Counter { hits = [3] };\n",
    "\tprint(growing.grow(twice(growing)));\n",
    "\tlet source = Counter { hits = [4, 5, 6] };\n",
    "\tprint(source.settle(twice(source)));\n",
    "\tlet again = Counter { hits = [7] };\n",
    "\tprint(generic(again));\n",
    "}\n",
);

/// F47 (UNSOUND until now): a `mut` PARAMETER a closure captures is one
/// binding the two frames share, and natively it was a COPY — `late(0)`
/// printed `0` where node prints `5`, and a closure that WROTE one was
/// refused by rustc as `FnMut`. A captured `mut` parameter is boxed like a
/// captured `mut` let and re-bound into its cell on entry. The shapes: a
/// read after the write, a closure writing it, a `List` pushed through a
/// capture, `mut self` written through a capture, and a closure's own `mut`
/// parameter captured by a closure inside it.
#[test]
fn a_captured_mut_parameter_is_shared_with_its_closure_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_mut_parameter.vl"),
        MUT_PARAMETER_PROBE,
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_mut_parameter.vl"),
        Verdict::Identical,
        "a closure's capture of a `mut` parameter must be the parameter itself on both backends"
    );
}

const MUT_PARAMETER_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "// A `mut` parameter a closure captures is one binding two frames share.\n",
    "fun late(mut n: i32): i32 {\n",
    "\tlet show = || n;\n",
    "\tn = 5;\n",
    "\tshow()\n",
    "}\n",
    "\n",
    "fun bump(mut n: i32): i32 {\n",
    "\tlet inc = || {\n",
    "\t\tn = n + 1;\n",
    "\t};\n",
    "\tinc();\n",
    "\tinc();\n",
    "\tn\n",
    "}\n",
    "\n",
    "fun collect(mut seen: List<i32>): usize {\n",
    "\tlet add = |value: i32| seen.push(value);\n",
    "\tadd(1);\n",
    "\tadd(2);\n",
    "\tseen.len()\n",
    "}\n",
    "\n",
    "struct Tally {\n",
    "\tcount: i32,\n",
    "}\n",
    "\n",
    "impl Tally {\n",
    "\tfun spend(mut self): i32 {\n",
    "\t\tlet take = || {\n",
    "\t\t\tself.count = self.count - 1;\n",
    "\t\t};\n",
    "\t\ttake();\n",
    "\t\tself.count\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tprint(late(0));\n",
    "\tprint(bump(0));\n",
    "\tprint(collect([9]));\n",
    "\tprint(Tally { count = 3 }.spend());\n",
    "\t// A closure's own `mut` parameter, captured by a closure inside it.\n",
    "\tlet outer = |mut total: i32| {\n",
    "\t\tlet add = |amount: i32| {\n",
    "\t\t\ttotal = total + amount;\n",
    "\t\t};\n",
    "\t\tadd(10);\n",
    "\t\tadd(20);\n",
    "\t\ttotal\n",
    "\t};\n",
    "\tprint(outer(1));\n",
    "}\n",
);

/// B435: a value erased where it lands in a `dyn` position that a generic
/// parameter or a literal hands it — `push` on a `List<dyn Src>` (the list's
/// own and a field's), an index assignment, `Some(value)` at an
/// `Option<dyn Src>`, and a generic struct literal under a `Boxed<dyn Src>`
/// annotation. Each printed a `TypeError` on JS before (the bare value
/// reached code reading a `(value, table)` pair) and was refused by rustc;
/// the two backends agree now.
#[test]
fn a_value_erased_at_a_bound_or_literal_dyn_position_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b435.vl"), B435_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b435.vl"),
        Verdict::Identical,
        "a value erased at a bound or literal `dyn` position must mean the same thing on both backends"
    );
}

const B435_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "trait Src {\n",
    "\tfun get(self): i32;\n",
    "}\n",
    "\n",
    "struct Root {\n",
    "\tn: i32,\n",
    "}\n",
    "\n",
    "impl Root with Src {\n",
    "\tfun get(self): i32 {\n",
    "\t\tself.n\n",
    "\t}\n",
    "}\n",
    "\n",
    "struct Boxed<T> {\n",
    "\tvalue: T,\n",
    "}\n",
    "\n",
    "struct Bag {\n",
    "\titems: List<dyn Src>,\n",
    "}\n",
    "\n",
    "fun total(objects: List<dyn Src>): i32 {\n",
    "\tmut sum = 0;\n",
    "\tfor object in objects {\n",
    "\t\tsum = sum + object.get();\n",
    "\t}\n",
    "\tsum\n",
    "}\n",
    "\n",
    "fun read(object: Option<dyn Src>): i32 {\n",
    "\tmatch object {\n",
    "\t\tSome(let found) => found.get(),\n",
    "\t\tNone => 0,\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet root = Root { n = 4 };\n",
    "\tprint(read(Some(root)));\n",
    "\tprint(read(Some(Root { n = 5 })));\n",
    "\tlet boxed: Boxed<dyn Src> = Boxed { value = Root { n = 7 } };\n",
    "\tprint(boxed.value.get());\n",
    "\tmut pushed: List<dyn Src> = [];\n",
    "\tpushed.push(Root { n = 8 });\n",
    "\tpushed.push(root);\n",
    "\tprint(total(pushed));\n",
    "\tmut bag = Bag { items = [] };\n",
    "\tbag.items.push(Root { n = 9 });\n",
    "\tprint(bag.items[0].get());\n",
    "\tmut slots: List<dyn Src> = [Root { n = 0 }];\n",
    "\tslots[0] = Root { n = 11 };\n",
    "\tprint(slots[0].get());\n",
    "}\n",
);

/// B418: a place or a `Shared` read reaching a binding through an `if` or
/// `match` arm is a copy on both backends. JS aliased it (the cell's later
/// write showed through the binding, and a `push` on the binding grew the
/// source) where the native build copied.
#[test]
fn a_place_chosen_by_a_branch_is_copied_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b418.vl"), B418_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b418.vl"),
        Verdict::Identical,
        "a place chosen by a branch must be copied the same way on both backends"
    );
}

const B418_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::shared::Shared;\n",
    "\n",
    "fun main() {\n",
    "\tlet s: Shared<List<i32>> = Shared::new([1, 2, 3]);\n",
    "\tlet flag = true;\n",
    "\tlet b = if flag { s.read() } else { [] };\n",
    "\ts.write().push(4);\n",
    "\tprint(b.len());\n",
    "\tlet c = match flag {\n",
    "\t\ttrue => s.read(),\n",
    "\t\tfalse => [],\n",
    "\t};\n",
    "\ts.write().push(5);\n",
    "\tprint(c.len());\n",
    "\tlet a: List<i32> = [1, 2];\n",
    "\tmut d = if flag { a } else { [] };\n",
    "\td.push(3);\n",
    "\tprint(a.len());\n",
    "\tprint(d.len());\n",
    "\tmut e: List<i32> = [];\n",
    "\te = if flag { a } else { [] };\n",
    "\te.push(9);\n",
    "\tprint(a.len());\n",
    "}\n",
);

/// B452: a sibling that lowers to statements — a block, an `if` or `match`
/// in value position — runs AFTER the siblings written before it, on JS as it
/// always did natively: call arguments, list and tuple elements, struct
/// fields in written order, binary operands, a method receiver, a `mut`
/// binding read before a block that writes it.
#[test]
fn a_sibling_that_lowers_to_statements_keeps_its_order_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b452.vl"), B452_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b452.vl"),
        Verdict::Identical,
        "sibling evaluation order must agree on both backends"
    );
}

const B452_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "struct Pair {\n",
    "\ta: i32,\n",
    "\tb: i32,\n",
    "}\n",
    "\n",
    "struct Acc {\n",
    "\tn: i32,\n",
    "}\n",
    "\n",
    "impl Acc {\n",
    "\tfun plus(self, k: i32): i32 {\n",
    "\t\tself.n + k\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun say(label: str, value: i32): i32 {\n",
    "\tprint(label);\n",
    "\tvalue\n",
    "}\n",
    "\n",
    "fun add(a: i32, b: i32): i32 {\n",
    "\ta + b\n",
    "}\n",
    "\n",
    "fun make(label: str): Acc {\n",
    "\tprint(label);\n",
    "\tAcc { n = 100 }\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tprint(add(say(\"a\", 1), { print(\"b-block\"); say(\"b\", 2) }));\n",
    "\tlet list = [say(\"x\", 1), { print(\"y-block\"); say(\"y\", 2) }];\n",
    "\tprint(list.len());\n",
    "\tlet p = Pair { b = say(\"b\", 2), a = { print(\"a-block\"); say(\"a\", 1) } };\n",
    "\tprint(p.a * 10 + p.b);\n",
    "\tlet c = true;\n",
    "\tprint(add(say(\"i\", 1), if c { print(\"j-if\"); say(\"j\", 2) } else { 0 }));\n",
    "\tprint(add(say(\"m\", 1), match Some(2) { Some(let v) => { print(\"n-match\"); say(\"n\", v) }, None => 0 }));\n",
    "\tmut x = 1;\n",
    "\tprint(add(x, { x = 10; x }));\n",
    "\tprint(say(\"l\", 1) + { print(\"r-block\"); say(\"r\", 2) });\n",
    "\tlet t = (say(\"p\", 1), { print(\"q-block\"); say(\"q\", 2) });\n",
    "\tprint(t.0 + t.1);\n",
    "\tprint(make(\"recv\").plus({ print(\"arg-block\"); 1 }));\n",
    "}\n",
);

/// B403: a bare `Holder::tag()` inside `impl Holder<type T: Label>` means
/// `Self::tag()`, so each `Holder<X>` reaches its own `X::label()`. The
/// native build emitted ONE instance for the unbound call and printed `A A`
/// for a `Holder<B>`; JS stopped with an internal error.
#[test]
fn a_bare_static_inside_its_own_impl_dispatches_per_instance_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b403.vl"), B403_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b403.vl"),
        Verdict::Identical,
        "a bare static inside its own impl must dispatch per instance on both backends"
    );
}

const B403_PROBE: &str = concat!(
    "trait Label { fun label(): str; }\n",
    "struct A {}\n",
    "impl A with Label { fun label(): str { \"A\" } }\n",
    "struct B {}\n",
    "impl B with Label { fun label(): str { \"B\" } }\n",
    "struct Holder<T> { v: T }\n",
    "impl Holder<type T: Label> {\n",
    "\tfun tag(): str { T::label() }\n",
    "\tfun show(self): str { Holder::tag() }\n",
    "\tfun show_self(self): str { Self::tag() }\n",
    "\tfun show_named(self): str { Holder<T>::tag() }\n",
    "}\n",
    "fun main() {\n",
    "\tprint(Holder { v = A {} }.show_self());\n",
    "\tprint(Holder { v = B {} }.show_named());\n",
    "\tprint(Holder { v = A {} }.show());\n",
    "\tprint(Holder { v = B {} }.show());\n",
    "}\n",
);

/// B419: a blanket over a SUPERTRAIT applies to a type whose one impl block
/// names the subtrait (B243's one-block form) — both backends find `pair`.
#[test]
fn a_blanket_over_a_supertrait_reaches_a_one_block_subtrait_impl_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b419.vl"), B419_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b419.vl"),
        Verdict::Identical,
        "a blanket over a supertrait must reach a one-block subtrait impl on both backends"
    );
}

const B419_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "trait Src<T> {\n",
    "\tfun get(self): T;\n",
    "}\n",
    "\n",
    "trait Sig<T> with Src<T> {\n",
    "\tfun label(self): str;\n",
    "}\n",
    "\n",
    "struct Cell<T> {\n",
    "\tv: T,\n",
    "}\n",
    "\n",
    "impl Cell<type T> with Sig<T> {\n",
    "\tfun get(self): T {\n",
    "\t\tself.v\n",
    "\t}\n",
    "\tfun label(self): str {\n",
    "\t\t\"cell\"\n",
    "\t}\n",
    "}\n",
    "\n",
    "impl type S: Src<type T> {\n",
    "\tfun pair(self): (T, T) {\n",
    "\t\t(self.get(), self.get())\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet c = Cell { v = 3 };\n",
    "\tlet (a, b) = c.pair();\n",
    "\tprint(a + b);\n",
    "}\n",
);

/// B423: a literal `if` arm or `match` leg takes its sibling's numeric type
/// (`u53` here) on both backends — the analyzer refused it before, and the
/// native build needs the literal typed to emit `0u64`.
#[test]
fn a_literal_arm_typed_by_its_sibling_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b423.vl"), B423_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b423.vl"),
        Verdict::Identical,
        "a literal arm typed by its sibling must mean the same thing on both backends"
    );
}

const B423_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun want(value: u53): u53 {\n",
    "\tvalue\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet n: u53 = 5;\n",
    "\tlet m = if n > 2 { n } else { 0 };\n",
    "\tlet k = if n > 2 { 0 } else { n };\n",
    "\tlet j = match n > 2 {\n",
    "\t\ttrue => n,\n",
    "\t\tfalse => 0,\n",
    "\t};\n",
    "\tprint(i\"{want(m)} {want(k)} {want(j)}\");\n",
    "}\n",
);

/// B457: a `Shared::read()` live across a CALL that writes the cell reads the
/// same on both backends — a notify loop whose subscriber adds a subscriber,
/// and a read handed by value to a callee that writes the cell.
#[test]
fn a_read_across_a_writing_call_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b457.vl"), B457_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b457.vl"),
        Verdict::Identical,
        "a read across a writing call must read the same on both backends"
    );
}

const B457_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::shared::Shared;\n",
    "\n",
    "fun direct(cell: Shared<List<i32>>, seen: List<i32>): usize {\n",
    "\tcell.write().push(9);\n",
    "\tseen.len()\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet log: Shared<List<str>> = Shared::new([]);\n",
    "\tlet subs: Shared<List<|| void>> = Shared::new([]);\n",
    "\tsubs.write().push(|| {\n",
    "\t\tlog.write().push(\"first\");\n",
    "\t\tsubs.write().push(|| log.write().push(\"late\"));\n",
    "\t});\n",
    "\tsubs.write().push(|| log.write().push(\"second\"));\n",
    "\tfor sub in subs.read() {\n",
    "\t\tsub();\n",
    "\t}\n",
    "\tprint(log.read().len());\n",
    "\tlet cell = Shared::new([1, 2]);\n",
    "\tprint(direct(cell, cell.read()));\n",
    "}\n",
);

/// B462: a tuple variant where a closure is expected is its constructor on
/// both backends — `Some` into `map`, a user variant with two payloads into a
/// two-parameter closure, the generics taken from the expected type.
#[test]
fn a_variant_standing_for_a_closure_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b462.vl"), B462_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b462.vl"),
        Verdict::Identical,
        "a variant coerced to a closure must build the same value on both backends"
    );
}

const B462_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "enum Shape {\n",
    "\tRect(i32, i32),\n",
    "\tDot,\n",
    "}\n",
    "\n",
    "fun build(make: |i32, i32| Shape): Shape {\n",
    "\tmake(2, 3)\n",
    "}\n",
    "\n",
    "fun area(shape: Shape): i32 {\n",
    "\tmatch shape {\n",
    "\t\tShape::Rect(let w, let h) => w * h,\n",
    "\t\tShape::Dot => 0,\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet xs = [1, 2].map(Some);\n",
    "\tprint(xs.len());\n",
    "\tprint(xs[1].unwrap());\n",
    "\tlet f: |i32| Option<i32> = Some;\n",
    "\tprint(f(3).unwrap());\n",
    "\tprint(area(build(Shape::Rect)));\n",
    "}\n",
);

/// B458: `Context::clear` lowers to a plain call of its body with the value
/// absent — a `get_safe` inside answers `None`, a closure minted inside keeps
/// the cleared state, a `run` inside re-establishes — the same on both
/// backends.
#[test]
fn a_cleared_context_reads_as_absent_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b458.vl"), B458_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b458.vl"),
        Verdict::Identical,
        "`clear` must read as absent the same way on both backends"
    );
}

const B458_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::context::Context;\n",
    "import std::option::Option::{ Some, None };\n",
    "\n",
    "let current: Context<i32> = Context::new();\n",
    "\n",
    "fun describe(): str {\n",
    "\tmatch current.get_safe() {\n",
    "\t\tSome(let value) => i\"some {value}\",\n",
    "\t\tNone => \"none\",\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tcurrent.run(7, || {\n",
    "\t\tprint(describe());\n",
    "\t\tcurrent.clear(|| {\n",
    "\t\t\tprint(describe());\n",
    "\t\t\tcurrent.run(2, || print(current.get()));\n",
    "\t\t});\n",
    "\t\tprint(describe());\n",
    "\t});\n",
    "\tlet answer = current.run(5, || current.clear(|| 3) + 1);\n",
    "\tprint(answer);\n",
    "}\n",
);

/// A142 S6: tracked reads — the `tracking` context threaded through every body
/// a pipe runs, the free `derive`'s stage, edges reconnected after each run,
/// an effect's tracked re-run, a selector's re-selection, a tracked diamond in
/// a turn, and `clear` as untrack — the same on both backends.
#[test]
fn tracked_reads_follow_the_same_dependencies_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_tracking.vl"), TRACKING_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_tracking.vl"),
        Verdict::Identical,
        "tracked reads must follow the same dependencies on both backends"
    );
}

const TRACKING_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::reactive::{\n",
    "\tFlushPolicy, Owner, Signal, SignalCell, Source, derive, run_with_owner, tracking, turn,\n",
    "};\n",
    "\n",
    "fun main() {\n",
    "\tlet flag: SignalCell<bool> = Signal::new(true);\n",
    "\tlet a: SignalCell<i32> = Signal::new(1);\n",
    "\tlet b: SignalCell<i32> = Signal::new(100);\n",
    "\tlet owner = Owner::new();\n",
    "\trun_with_owner(owner, || {\n",
    "\t\tderive(|| if flag.track() { a.track() } else { b.track() })\n",
    "\t\t\t.effect(|value: i32| print(i\"branch {value}\"));\n",
    "\t\ta.effect(|value: i32| {\n",
    "\t\t\tlet other = b.track();\n",
    "\t\t\tprint(i\"effect {value} {other}\");\n",
    "\t\t});\n",
    "\t});\n",
    "\ta.set(2);\n",
    "\tb.set(200);\n",
    "\tflag.set(false);\n",
    "\ta.set(3);\n",
    "\tlet stage = a.derive(|x: i32| x + b.track()).memo();\n",
    "\tb.set(300);\n",
    "\tprint(i\"stage {stage.get()}\");\n",
    "\tlet picked = flag.switch(|on: bool| {\n",
    "\t\tlet base = b.track();\n",
    "\t\ta.derive(|x: i32| if on { x } else { x + base })\n",
    "\t}).memo();\n",
    "\tb.set(1000);\n",
    "\tprint(i\"picked {picked.get()}\");\n",
    "\tlet tens = a.derive(|x: i32| x * 10).memo();\n",
    "\tlet hundreds = a.derive(|x: i32| x * 100).memo();\n",
    "\tlet sums = Owner::new();\n",
    "\trun_with_owner(sums, || {\n",
    "\t\tderive(|| tens.track() + hundreds.track()).effect(|sum: i32| print(i\"sum {sum}\"));\n",
    "\t});\n",
    "\tturn(FlushPolicy::AtEnd, || a.set(4));\n",
    "\tlet untracked = derive(|| a.track() + tracking.clear(|| b.get())).memo();\n",
    "\tb.set(5);\n",
    "\tprint(i\"untracked {untracked.get()}\");\n",
    "\ta.set(6);\n",
    "\tprint(i\"untracked {untracked.get()}\");\n",
    "\tsums.dispose();\n",
    "\towner.dispose();\n",
    "}\n",
);

/// B475: a pipe over a `dyn Source<T>` STARTS the object through a `Flow`
/// bound, so the object's table carries `Flow::start` (a supertrait member) —
/// a `derive` sealed with `.cell()`, one consumed by `on_change`, one read with
/// `.sample()`, and the total join `switch(|inner| inner)` over a cell of
/// objects. JS threw "start is not a function"; both backends agree now.
#[test]
fn a_pipe_over_a_source_object_starts_it_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b475.vl"), B475_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b475.vl"),
        Verdict::Identical,
        "a pipe over a source object must start it the same way on both backends"
    );
}

const B475_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::reactive::{ SignalCell, Source };\n",
    "\n",
    "fun main() {\n",
    "\tlet cell = SignalCell::new(1);\n",
    "\tlet object: dyn Source<i32> = cell;\n",
    "\tlet cached = object.derive(|n: i32| n + 1).cell();\n",
    "\tlet watch = object.derive(|n: i32| n * 10).on_change(|n: i32| print(i\"saw {n}\"));\n",
    "\tlet outer = SignalCell::new(object);\n",
    "\tlet flat = outer.switch(|inner: dyn Source<i32>| inner).memo();\n",
    "\tcell.set(5);\n",
    "\tprint(i\"{object.derive(|n: i32| n + 1).sample()} {cached.get()} {flat.get()}\");\n",
    "\twatch.dispose();\n",
    "}\n",
);

/// B478: a named function and a variant at a `context`-typed closure position
/// — `count.derive(Some)`, `count.derive(double)`, `count.effect(show)`. JS
/// ignores the hidden context arguments the caller appends; natively the value
/// is adapted to the position's arity (the function item alone was refused by
/// rustc, E0593).
#[test]
fn a_named_function_at_a_context_typed_position_runs_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b478.vl"), B478_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b478.vl"),
        Verdict::Identical,
        "a named function or a variant must stand for a context-typed body on both backends"
    );
}

const B478_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::option::Option::{ self, None, Some };\n",
    "import std::reactive::{ Owner, SignalCell, Source, run_with_owner };\n",
    "\n",
    "fun show(value: i32) {\n",
    "\tprint(i\"show {value}\");\n",
    "}\n",
    "\n",
    "fun double(value: i32): i32 {\n",
    "\tvalue * 2\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet count = SignalCell::new(4);\n",
    "\tlet wrapped = count.derive(Some).memo();\n",
    "\tlet doubled = count.derive(double).memo();\n",
    "\tlet owner = Owner::new();\n",
    "\trun_with_owner(owner, || {\n",
    "\t\tcount.effect(show);\n",
    "\t});\n",
    "\tcount.set(5);\n",
    "\tprint(wrapped.get().unwrap() + doubled.get());\n",
    "\towner.dispose();\n",
    "}\n",
);

/// B480 + B484: a binder bound through a trait ARGUMENT, reached through an
/// object — `switch`'s `U` from a `dyn Flow<i32>` selector with nothing
/// written, and a collection pipe over a list of `dyn Source<Option<str>>`
/// whose `filter_map(|source| source)` binds `R: IntoFlow<Option<U>>`'s `U`
/// from the object's own trait argument. Natively the second reached the
/// emitter with `U` unbound and was refused by name.
#[test]
fn a_binder_through_an_objects_trait_argument_is_bound_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b480.vl"), B480_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b480.vl"),
        Verdict::Identical,
        "a binder through an object's trait argument must bind on both backends"
    );
}

const B480_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::option::Option::{ self, None, Some };\n",
    "import std::reactive::{ Flow, ListCell, Signal, SignalCell, Source, comp };\n",
    "\n",
    "fun arm(on: bool, count: SignalCell<i32>): dyn Flow<i32> {\n",
    "\tif on {\n",
    "\t\tcount.derive(|value| value * 100)\n",
    "\t} else {\n",
    "\t\tcount\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet flag = Signal::new(true);\n",
    "\tlet count = Signal::new(1);\n",
    "\tlet picked = flag.switch(|on| arm(on, count)).memo();\n",
    "\tprint(picked.get());\n",
    "\tflag.set(false);\n",
    "\tcount.set(3);\n",
    "\tprint(picked.get());\n",
    "\tlet a: SignalCell<Option<str>> = Signal::new(Some(\"a\"));\n",
    "\tlet b: SignalCell<Option<str>> = Signal::new(None);\n",
    "\tlet first: dyn Source<Option<str>> = a;\n",
    "\tlet second: dyn Source<Option<str>> = b;\n",
    "\tlet sources: ListCell<dyn Source<Option<str>>> = ListCell::of([first, second]);\n",
    "\tlet (loaded, scope) = comp(|| sources.filter_map(|source| source).memo());\n",
    "\tprint(loaded.get().len());\n",
    "\tb.set(Some(\"b\"));\n",
    "\tlet names = loaded.get();\n",
    "\tprint(names[0] + names[1]);\n",
    "\tscope.dispose();\n",
    "}\n",
);

/// B482: an injected callback called inside `clear` of its own context gets
/// the CLEARED state — `get_safe` is `None` there and the bare value's `Some`
/// from a plain call through the same position — and natively the position's
/// closure type carries the context as an `Option` (`cleared_clause_contexts`),
/// which every literal landing there agrees with, beside a `run` body and a
/// plain injected position that stay bare.
#[test]
fn a_callback_called_inside_clear_reads_the_context_as_absent_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b482.vl"), B482_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b482.vl"),
        Verdict::Identical,
        "a cleared callback must read the context as absent on both backends"
    );
}

const B482_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::context::Context;\n",
    "import std::option::Option::{ Some, None };\n",
    "\n",
    "let current: Context<i32> = Context::new();\n",
    "\n",
    "fun describe(): str {\n",
    "\tmatch current.get_safe() {\n",
    "\t\tSome(let value) => i\"some {value}\",\n",
    "\t\tNone => \"none\",\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun notify(callback: (|i32| void) context current) {\n",
    "\tcurrent.clear(|| callback(1));\n",
    "}\n",
    "\n",
    "fun twice(callback: (|i32| void) context current) {\n",
    "\tcallback(1);\n",
    "\tcurrent.clear(|| callback(2));\n",
    "}\n",
    "\n",
    "fun plain(callback: (|i32, i32| void) context current) {\n",
    "\tcallback(3, 4);\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tcurrent.run(7, || {\n",
    "\t\tnotify(|n: i32| print(i\"{n} {describe()}\"));\n",
    "\t\ttwice(|n: i32| print(i\"{n} {describe()}\"));\n",
    "\t\tplain(|n: i32, m: i32| print(n + m + current.get()));\n",
    "\t});\n",
    "}\n",
);

/// J7 + M92: a spawn in a literal born under `context ambient_nursery` — a
/// user-written clause, and a pipe body's — is OWNED by the nursery its caller
/// injects: cancelled with it, and its cancellation absorbed (native-44 found JS
/// reporting two "unhandled task error … AbortError" lines here and native none).
/// A run that starts no task hands its nursery on (`nurseries=0`). Stdout the
/// same on both backends, and stderr empty on both.
#[test]
fn an_injected_nurserys_spawn_is_owned_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_j7.vl"), J7_PROBE).expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_j7.vl"),
        Verdict::Identical,
        "an injected nursery's spawn must be owned the same way on both backends"
    );
    for backend in [None, Some("rust")] {
        let mut command = vilan(&staged);
        command.arg("run");
        if let Some(backend) = backend {
            command.args(["--backend", backend]);
        }
        let output = command
            .arg("native_probe_j7.vl")
            .output()
            .expect("run the probe");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "nurseries=0 value=12\nfetched 3\ndone\n",
            "backend {backend:?}"
        );
        assert!(
            output.stderr.is_empty(),
            "a cancelled owned task reports nothing (backend {backend:?}): {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

const J7_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::reactive::{ Owner, Signal, SignalCell, Source, run_nurseries_allocated, run_with_owner };\n",
    "import std::task::{ ambient_nursery, detached_nursery };\n",
    "import std::time::sleep;\n",
    "\n",
    "fun single(body: (|| void) context ambient_nursery) {\n",
    "\tlet held = detached_nursery();\n",
    "\tambient_nursery.run(held, body);\n",
    "\theld.cancel();\n",
    "}\n",
    "\n",
    "fun fetch_later(value: i32) {\n",
    "\tlet _pending = async {\n",
    "\t\tsleep(30);\n",
    "\t\tprint(i\"fetched {value}\");\n",
    "\t};\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tsingle(|| {\n",
    "\t\tlet _task = async {\n",
    "\t\t\tsleep(20);\n",
    "\t\t\tprint(\"single survived\");\n",
    "\t\t};\n",
    "\t});\n",
    "\tlet id: SignalCell<i32> = Signal::new(1);\n",
    "\tlet boundary = Owner::new();\n",
    "\tlet sealed = run_with_owner(boundary, || id.derive(|value: i32| value * 4).memo());\n",
    "\tlet before = run_nurseries_allocated();\n",
    "\tid.set(2);\n",
    "\tid.set(3);\n",
    "\tprint(i\"nurseries={run_nurseries_allocated() - before} value={sealed.get()}\");\n",
    "\trun_with_owner(boundary, || {\n",
    "\t\tid.effect(|value: i32| fetch_later(value));\n",
    "\t});\n",
    "\tid.set(4);\n",
    "\tid.set(3);\n",
    "\tsleep(120);\n",
    "\tboundary.dispose();\n",
    "\tprint(\"done\");\n",
    "}\n",
);

/// A146: `ListCell` and `KeyedCell` name their identity, so a body that tracks
/// one on every run keeps ONE edge on it (`attaches=1`) on both backends. (The
/// mirror half, `RemoteSource` and `KeyedSource` over `duplex_pair`, is
/// `inference::tracking`'s A146 program, natively in
/// `an_in_process_mirror_program_is_identical_on_both_backends` since F74.)
#[test]
fn a_tracked_list_or_keyed_cell_keeps_one_edge_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_a146.vl"), A146_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_a146.vl"),
        Verdict::Identical,
        "a tracked list or keyed cell must keep one edge the same way on both backends"
    );
    let output = vilan(&staged)
        .args(["run", "native_probe_a146.vl"])
        .output()
        .expect("run the probe");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "list attaches=1\nkeyed cell attaches=1\n5 2 1\n",
        "one edge per source across six runs (red before A146: `attaches=6`)"
    );
}

const A146_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::option::Option::{ self, None, Some };\n",
    "import std::reactive::{ Signal, SignalCell, Source, Subscriber, Subscription, derive };\n",
    "import std::delta::ListCell;\n",
    "import std::rpc::{ KeyedCell, Keyed };\n",
    "import std::shared::Shared;\n",
    "\n",
    "// Counts the edges a tracked read attaches to the wrapped source, and answers the\n",
    "// wrapped source's identity.\n",
    "struct Counted<S> {\n",
    "\tinner: S,\n",
    "\tattaches: Shared<i32>,\n",
    "}\n",
    "\n",
    "impl Counted<type S: Source<type T>> with Source<T> {\n",
    "\tfun get(self): T {\n",
    "\t\tself.inner.get()\n",
    "\t}\n",
    "\n",
    "\tfun on_settle(self, subscriber: Subscriber): Subscription {\n",
    "\t\tself.attaches.write() = self.attaches.read() + 1;\n",
    "\t\tself.inner.on_settle(subscriber)\n",
    "\t}\n",
    "\n",
    "\tfun identity(self): Option<i32> {\n",
    "\t\tself.inner.identity()\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun counted<S>(inner: S): Counted<S> {\n",
    "\tCounted { inner, attaches = Shared::new(0) }\n",
    "}\n",
    "\n",
    "[derive(Wire)]\n",
    "struct Row {\n",
    "\tid: str,\n",
    "\tn: i32,\n",
    "}\n",
    "\n",
    "impl Row with Keyed<str> {\n",
    "\tfun key(self): str {\n",
    "\t\tself.id\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet list = counted(ListCell::of([1, 2]));\n",
    "\tlet keyed: Counted<KeyedCell<str, Row>> = counted(KeyedCell::new([Row { id = \"a\", n = 1 }]));\n",
    "\tlet trigger: SignalCell<i32> = Signal::new(0);\n",
    "\tlet sealed = derive(|| {\n",
    "\t\ti\"{trigger.track()} {list.track().len()} {keyed.track().len()}\"\n",
    "\t}).memo();\n",
    "\tmut step = 1;\n",
    "\tfor step <= 5 {\n",
    "\t\ttrigger.set(step);\n",
    "\t\tstep += 1;\n",
    "\t}\n",
    "\tprint(i\"list attaches={list.attaches.read()}\");\n",
    "\tprint(i\"keyed cell attaches={keyed.attaches.read()}\");\n",
    "\tprint(sealed.get());\n",
    "}\n",
    "\n",
    "main();\n",
);

/// B482's std half: `on_change`, `sub` and `effect_on_change` call their
/// callback under `tracking.clear(..)`, so a callback minted inside an effect
/// body reads `tracking` as absent — the same on both backends.
#[test]
fn a_base_callback_runs_with_tracking_cleared_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b482_std.vl"), B482_STD_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b482_std.vl"),
        Verdict::Identical,
        "a base callback must run with tracking cleared the same way on both backends"
    );
}

const B482_STD_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::reactive::{ Owner, Signal, SignalCell, Source, run_with_owner, tracking };\n",
    "\n",
    "fun main() {\n",
    "\tlet count: SignalCell<i32> = Signal::new(1);\n",
    "\tlet other: SignalCell<i32> = Signal::new(1);\n",
    "\tlet owner = Owner::new();\n",
    "\trun_with_owner(owner, || {\n",
    "\t\tcount.effect(|_value: i32| {\n",
    "\t\t\tprint(i\"body {tracking.get_safe().is_some()}\");\n",
    "\t\t\tlet _changed = other.on_change(|value| print(i\"on_change {value} {tracking.get_safe().is_none()}\"));\n",
    "\t\t\tlet _subbed = other.sub(|value| print(i\"sub {value} {tracking.get_safe().is_none()}\"));\n",
    "\t\t\tother.effect_on_change(|value| print(i\"effect_on_change {value} {tracking.get_safe().is_none()}\"));\n",
    "\t\t});\n",
    "\t});\n",
    "\tother.set(2);\n",
    "\towner.dispose();\n",
    "}\n",
);

/// B470: a `Drop`-free resource erased into a `[resource] trait`'s object.
/// The analyzer admits it (pinned on JS in `inference::dyn_objects`); the
/// native half — building the erased pair for a resource without cloning — is
/// native-44's, so until it lands the native build must REFUSE BY NAME and
/// never hand rustc something wrong. When it lands this pin expects identity.
#[test]
fn a_resource_erased_into_a_resource_traits_object_is_identical_or_refused_by_name() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b470.vl"), B470_PROBE)
        .expect("write the probe program");
    match compare(&staged, "native_probe_b470.vl") {
        Verdict::Identical | Verdict::Refused(_) => {}
        Verdict::Broken(detail) => panic!("the native build was accepted and wrong: {detail}"),
    }
}

const B470_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "[resource]\n",
    "trait Run {\n",
    "\tfun run(own self): i32;\n",
    "}\n",
    "\n",
    "[resource]\n",
    "struct Node {\n",
    "\tv: i32,\n",
    "}\n",
    "\n",
    "impl Node with Run {\n",
    "\tfun run(own self): i32 {\n",
    "\t\tself.v + 1\n",
    "\t}\n",
    "}\n",
    "\n",
    "struct Plain {\n",
    "\tv: i32,\n",
    "}\n",
    "\n",
    "impl Plain with Run {\n",
    "\tfun run(own self): i32 {\n",
    "\t\tself.v * 10\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun pick(on: bool): dyn Run {\n",
    "\tif on {\n",
    "\t\tNode { v = 1 }\n",
    "\t} else {\n",
    "\t\tPlain { v = 2 }\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tprint(pick(true).run());\n",
    "\tprint(pick(false).run());\n",
    "}\n",
);

/// B430: the native build does not yet re-build a tuple erased element-wise
/// (the JS emitter does, by projection) — it REFUSES by name rather than
/// handing rustc a bare struct where a `Dyn` is wanted.
#[test]
fn a_tuple_erased_elementwise_is_refused_by_name_natively() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b430.vl"), B430_PROBE)
        .expect("write the probe program");
    match compare(&staged, "native_probe_b430.vl") {
        Verdict::Refused(reason) => assert!(
            reason.contains("a tuple value erased element-wise"),
            "refused for another reason: {reason}"
        ),
        other => panic!("expected a refusal by name, got {other:?}"),
    }
}

const B430_PROBE: &str = concat!(
    "import std::io::print;\n",
    "trait Src {\n",
    "\tfun get(self): i32;\n",
    "}\n",
    "struct Root {\n",
    "\tn: i32,\n",
    "}\n",
    "impl Root with Src {\n",
    "\tfun get(self): i32 {\n",
    "\t\tself.n\n",
    "\t}\n",
    "}\n",
    "fun pair(p: (dyn Src, dyn Src)): i32 {\n",
    "\tp.0.get() + p.1.get()\n",
    "}\n",
    "fun main() {\n",
    "\tlet t = (Root { n = 1 }, Root { n = 2 });\n",
    "\tprint(pair(t));\n",
    "}\n",
);

/// B424 door (b): `or_else`'s free `F` takes the input's error type, so the
/// native build has a type to emit — it refused by name before ("a value of
/// an unbound generic type parameter (parameter 1 of `or_else`)").
#[test]
fn or_else_with_an_ok_only_closure_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b424.vl"), B424_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b424.vl"),
        Verdict::Identical,
        "`or_else` with an Ok-only closure must mean the same thing on both backends"
    );
}

const B424_PROBE: &str = concat!(
    "import std::io::print;\n",
    "fun main() {\n",
    "\tlet err: Result<i32, str> = Err(\"bad\");\n",
    "\tlet fixed = err.or_else(|e| Ok(7));\n",
    "\tprint(fixed.unwrap_or(0));\n",
    "\tlet fixed2 = err.or_else(|_| Ok(8));\n",
    "\tprint(fixed2.unwrap_or(0));\n",
    "}\n",
);

/// F18 slice 1: the emitter reaches `vilan_rt::http`.
///
/// A `std::http` server program EMITS, and what comes out names the runtime's
/// own calls rather than a refusal — `create_server`, the bound `listen`, the
/// request body read, and the response's status/header/end. Twenty-three of
/// `std::http`'s twenty-five raw `node:http` bindings are answered by
/// `vilan_rt::http`; the two that are not are `NodeRequest::headers` and
/// `NodeSocket::remoteAddress`, which answer a `JsonValue` and are Order 40's,
/// and this program reaches neither.
///
/// It asserts the CALLS and not merely that the emit succeeded, because an
/// emitter that refused every binding under the census's `unimplemented!()`
/// would also "succeed". The slice's EXIT — the same program built, run, and
/// answering a GET over a real socket — is
/// [`a_native_std_http_server_answers_a_get_over_a_real_socket`]; this pin is
/// the cheap half, and it is what says WHICH bindings the exit went through.
#[test]
fn a_std_http_server_emits_calls_into_the_native_runtime() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_http.vl"), HTTP_PROBE)
        .expect("write the probe program");
    let output = vilan(&staged)
        .args([
            "build",
            "--backend",
            "rust",
            "--stdout",
            "native_probe_http.vl",
        ])
        .output()
        .expect("build the probe");
    assert!(
        output.status.success(),
        "the http probe was refused:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source = String::from_utf8_lossy(&output.stdout);
    for needle in [
        "vilan_rt::http::create_server",
        "vilan_rt::http::read_request_bytes",
        ").listen(",
        ").address()",
        ").port()",
        ").set_status_code(",
        ").set_header(",
        ").end(",
        ").end_bytes(",
        "vilan_rt::http::Request",
        "vilan_rt::http::Response",
        "vilan_rt::bytes::Bytes",
    ] {
        assert!(
            source.contains(needle),
            "the emitted server must reach `{needle}`:\n{source}"
        );
    }
    assert!(
        !source.contains("unimplemented!()"),
        "a build emit must never carry a census placeholder:\n{source}"
    );
}

/// A `std::http` server built from the struct directly, which is the smallest
/// program that reaches the whole `node:http` surface `Server::start` binds.
///
/// `Server::builder()` is the shipped spelling and it is NOT used here: its
/// `build()` folds the rpc service list, which sorts (a backed enum plus the
/// `ListSortBy` intrinsic, both other lanes' items) and reaches `serve_build`'s
/// conditional-GET arm (`std::json`'s host type, Order 40's). The literal
/// reaches the same server.
const HTTP_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::http::{ Server, Response };\n",
    "import std::option::Option::None;\n",
    "\n",
    "fun main() {\n",
    "\tlet server = Server {\n",
    "\t\tport = 0,\n",
    "\t\trequest_handler = |request| Response::builder()\n",
    "\t\t\t.set_header(\"Content-Type\", \"text/plain\")\n",
    "\t\t\t.body(\"hello\\n\")\n",
    "\t\t\t.build(),\n",
    "\t\ton_start = |started| print(i\"vilan-test-port={started.port()}\"),\n",
    "\t\ton_stop = |stopped| {},\n",
    "\t\tupgrade_handler = None,\n",
    "\t\tnode = None,\n",
    "\t};\n",
    "\tserver.start();\n",
    "}\n",
);

/// **F18 slice 1's EXIT**: a `std::http` server compiled with `--backend rust`
/// runs as a native binary and answers a `GET /` over a real socket, and the JS
/// twin answers the same thing.
///
/// The whole slice is measured here. Everything else about it — the
/// dependency-free HTTP/1.1 server in `vilan-rt`, the `IoSource` turn, the
/// twenty-three `node:http` bindings, the executor's free list — exists so that
/// this program serves a request, and a runtime whose own unit tests pass while
/// the compiler cannot reach it would be a runtime nobody can use.
///
/// **What is compared, and what cannot be.** The status line, the header the
/// PROGRAM set, and the body, byte for byte on both legs. Not the whole
/// response: node adds a `Date`, which changes every second, and node and this
/// server order `Connection` and `Content-Length` differently — two facts
/// written down rather than normalised away, because a reader deserves to know
/// the comparison is not the whole wire. stdout IS compared whole, and it is
/// the port announcement, which is the same line from both.
///
/// **The ordering is the harness's own, not a sleep.** The SERVER binds port 0
/// and announces the number it got; the fetch cannot start before that line has
/// arrived, because the line is where the number comes from. There is no
/// bind-release-rebind window (`support/port.rs`'s N40 finding) and no sleep
/// standing in for a happens-before.
#[test]
fn a_native_std_http_server_answers_a_get_over_a_real_socket() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_http.vl"), HTTP_PROBE)
        .expect("write the probe program");

    // The native leg: build, then run the BINARY rather than `vilan run`, so
    // the child this test kills is the server itself and not a parent that
    // would outlive it.
    let built = vilan(&staged)
        .args(["build", "--backend", "rust", "native_probe_http.vl"])
        .output()
        .expect("build the server natively");
    assert!(
        built.status.success(),
        "the native leg did not build:\n{}{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    let binary = String::from_utf8_lossy(&built.stdout)
        .lines()
        .find_map(|line| line.split(" -> ").nth(1).map(str::to_string))
        .expect("`vilan build` says where the binary is");
    let native = ServedRequest::take(Command::new(staged.join(&binary)));

    // The JS leg, the same program, the same request.
    let bundled = vilan(&staged)
        .args(["build", "native_probe_http.vl"])
        .output()
        .expect("build the server for node");
    assert!(
        bundled.status.success(),
        "the JS leg did not build:\n{}",
        String::from_utf8_lossy(&bundled.stderr)
    );
    let mut node = Command::new("node");
    node.current_dir(&staged).arg("native_probe_http.mjs");
    let javascript = ServedRequest::take(node);

    assert_eq!(
        native.status, javascript.status,
        "the two backends must answer the same status line"
    );
    assert_eq!(
        native.status, "HTTP/1.1 200 OK",
        "and it is a 200 — a pin that agreed on a 500 would agree about nothing"
    );
    assert_eq!(
        native.body, javascript.body,
        "the two backends must answer the same body"
    );
    assert_eq!(native.body, "hello\n", "and it is the handler's own body");
    // The header the PROGRAM set goes out on both. Node's `Date` and the order
    // it writes `Connection`/`Content-Length` in are its own; see this test's
    // header comment.
    for leg in [&native, &javascript] {
        assert!(
            leg.headers
                .iter()
                .any(|line| line == "Content-Type: text/plain"),
            "the program's header must reach the wire: {:?}",
            leg.headers
        );
        assert!(
            leg.headers.iter().any(|line| line == "Content-Length: 6"),
            "a buffered body declares its length: {:?}",
            leg.headers
        );
    }
    assert_eq!(
        native.announced_line.split('=').next(),
        javascript.announced_line.split('=').next(),
        "both legs announce through the same `on_start`"
    );
}

/// **F18 slice 2's EXIT**: a program with the SHAPE of kolt's server leg —
/// a SQLite store, a hashed password, an `/api/login` route that decodes a POST
/// body and answers a `[derive(Json)]` outcome, and a shell for every other
/// path — runs as a native binary and answers a login over a real socket.
///
/// **Why a shape and not the file.** Kolt is read-only for this tree and is
/// never copied into it; what is reproduced is the STRUCTURE the slice had to
/// carry, which is what the exit is measuring. Everything the slice built is on
/// the path: `std::json` (the derived encode, and `List<str>::from_json` over
/// the request body), `std::db` through the separate `vilan-rt-sqlite` crate,
/// `std::crypto`'s SHA-256, `Bytes` and `TextDecoder`, `std::http` over a real
/// socket, and a `for` over an `Iterator` impl (`Bytes::to_hex` walks a
/// `Range`).
///
/// **What is compared.** Three exchanges, each byte for byte on both legs: a
/// good login, a bad one, and the shell — status line, the header the program
/// set, `Content-Length` and the body. NOT compared, for Order 39's reasons
/// written at [`a_native_std_http_server_answers_a_get_over_a_real_socket`]:
/// node's `Date`, and the order the two write `Connection`/`Content-Length`.
///
/// **Non-vacuous by its content, not by its exit code.** The bodies are
/// asserted verbatim, and the two logins differ only in the password — so a
/// server that answered a constant, or one whose hash comparison always held,
/// fails on the second exchange.
#[test]
fn the_kolt_server_shape_answers_a_login_over_a_real_socket() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_kolt.vl"), KOLT_SHAPE_PROBE)
        .expect("write the probe program");

    let built = vilan(&staged)
        .args(["build", "--backend", "rust", "native_probe_kolt.vl"])
        .output()
        .expect("build the server natively");
    assert!(
        built.status.success(),
        "the native leg did not build:\n{}{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    // Order 39's R1, asserted rather than assumed: the SQLite crate is named by
    // the manifest of a program that reaches `std::db`.
    let manifest = std::fs::read_to_string(
        staged
            .join("dist")
            .join("native")
            .join("native_probe_kolt")
            .join("Cargo.toml"),
    )
    .expect("read the generated manifest");
    assert!(
        manifest.contains("vilan-rt-sqlite"),
        "a program reaching `std::db` depends on the SQLite crate:\n{manifest}"
    );
    let binary = String::from_utf8_lossy(&built.stdout)
        .lines()
        .find_map(|line| line.split(" -> ").nth(1).map(str::to_string))
        .expect("`vilan build` says where the binary is");
    let native = ServedLogin::take(Command::new(staged.join(&binary)));

    let bundled = vilan(&staged)
        .args(["build", "native_probe_kolt.vl"])
        .output()
        .expect("build the server for node");
    assert!(
        bundled.status.success(),
        "the JS leg did not build:\n{}",
        String::from_utf8_lossy(&bundled.stderr)
    );
    let mut node = Command::new("node");
    node.current_dir(&staged).arg("native_probe_kolt.mjs");
    let javascript = ServedLogin::take(node);

    assert_eq!(
        native.exchanges, javascript.exchanges,
        "the two backends must answer the same three exchanges"
    );
    let expected = [
        (
            "HTTP/1.1 200 OK",
            "Content-Type: application/json",
            "{\"ok\":true,\"message\":\"welcome ada\"}",
        ),
        (
            "HTTP/1.1 200 OK",
            "Content-Type: application/json",
            "{\"ok\":false,\"message\":\"wrong password\"}",
        ),
        (
            "HTTP/1.1 200 OK",
            "Content-Type: text/html",
            "<!doctype html><title>shape</title>",
        ),
    ];
    for (answered, (status, header, body)) in native.exchanges.iter().zip(expected) {
        assert_eq!(answered.status, status);
        assert_eq!(answered.body, body);
        assert!(
            answered.headers.iter().any(|line| line == header),
            "the program's header must reach the wire: {:?}",
            answered.headers
        );
        assert!(
            answered
                .headers
                .iter()
                .any(|line| line == &format!("Content-Length: {}", body.len())),
            "a buffered body declares its length: {:?}",
            answered.headers
        );
    }
}

const KOLT_SHAPE_PROBE: &str = concat!(
    "// THE SHAPE of kolt's server leg (`src/server.vl` + the `KoltAuth` half of\n",
    "// `src/store.vl`), written here from scratch: one `Server` over a SQLite\n",
    "// store, an `/api/login` route that reads a POST body, checks a password\n",
    "// against a hashed row and answers a `[derive(Json)]` outcome, and a shell\n",
    "// for every other path. Kolt's own files are never copied into this tree.\n",
    "import std::bytes::encode_utf8;\n",
    "import std::crypto::sha256;\n",
    "import std::db::Database;\n",
    "import std::http::{ Request, Response, Server };\n",
    "import std::io::print;\n",
    "import std::json::{ FromJson, Json };\n",
    "import std::option::Option::None;\n",
    "\n",
    "[derive(Json)]\n",
    "struct LoginOutcome {\n",
    "\tok: bool,\n",
    "\tmessage: str,\n",
    "}\n",
    "\n",
    "let store = open_store();\n",
    "\n",
    "fun open_store(): Database {\n",
    "\tlet db = Database::open(\":memory:\");\n",
    "\tdb.exec(\"CREATE TABLE account (id INTEGER PRIMARY KEY, username TEXT NOT NULL UNIQUE, hash TEXT NOT NULL)\");\n",
    "\tdb\n",
    "}\n",
    "\n",
    "async fun hash_password(username: str, password: str): str {\n",
    "\t// Kolt hashes with node:crypto's PBKDF2; the SHAPE is the same — a salted\n",
    "\t// digest of the password, stored beside the account.\n",
    "\tsha256(encode_utf8(username + \":\" + password)).to_hex()\n",
    "}\n",
    "\n",
    "async fun register(username: str, password: str): LoginOutcome {\n",
    "\tlet hashed = hash_password(username, password);\n",
    "\tstore.prepare(\"INSERT INTO account (username, hash) VALUES (?, ?)\").run([username, hashed]);\n",
    "\tLoginOutcome { ok = true, message = \"registered\" }\n",
    "}\n",
    "\n",
    "async fun login(username: str, password: str): LoginOutcome {\n",
    "\tlet hashed = hash_password(username, password);\n",
    "\tmatch store.prepare(\"SELECT hash FROM account WHERE username = ?\").first([username]) {\n",
    "\t\tSome(let row) => if row.text(\"hash\") == hashed {\n",
    "\t\t\tLoginOutcome { ok = true, message = \"welcome \" + username }\n",
    "\t\t} else {\n",
    "\t\t\tLoginOutcome { ok = false, message = \"wrong password\" }\n",
    "\t\t},\n",
    "\t\tNone => LoginOutcome { ok = false, message = \"no such account\" },\n",
    "\t}\n",
    "}\n",
    "\n",
    "async fun main() {\n",
    "\tregister(\"ada\", \"lovelace1\");\n",
    "\tlet server = Server {\n",
    "\t\tport = 0,\n",
    "\t\trequest_handler = |request| answer(request),\n",
    "\t\ton_start = |started| print(i\"vilan-test-port={started.port()}\"),\n",
    "\t\ton_stop = |stopped| {},\n",
    "\t\tupgrade_handler = None,\n",
    "\t\tnode = None,\n",
    "\t};\n",
    "\tserver.start();\n",
    "}\n",
    "\n",
    "async fun answer(request: Request): Response {\n",
    "\tif request.path() == \"/api/login\" && request.method() == \"POST\" {\n",
    "\t\tlet pair = List<str>::from_json(request.body()).unwrap_or([]);\n",
    "\t\tlet outcome = if pair.len() == 2 {\n",
    "\t\t\tlogin(pair.get(0).unwrap_or(\"\"), pair.get(1).unwrap_or(\"\"))\n",
    "\t\t} else {\n",
    "\t\t\tLoginOutcome { ok = false, message = \"malformed call\" }\n",
    "\t\t};\n",
    "\t\tret Response::builder()\n",
    "\t\t\t.set_header(\"Content-Type\", \"application/json\")\n",
    "\t\t\t.body(outcome.to_json())\n",
    "\t\t\t.build();\n",
    "\t}\n",
    "\tResponse::builder()\n",
    "\t\t.set_header(\"Content-Type\", \"text/html\")\n",
    "\t\t.body(\"<!doctype html><title>shape</title>\")\n",
    "\t\t.build()\n",
    "}\n",
);

/// **F18 slice 3**: `Server::builder()` — the shipped spelling, which every
/// server before this pin had to avoid for the `Server` struct literal — builds
/// natively and serves a build over a real socket, byte-for-byte with node,
/// INCLUDING `serve_build`'s conditional-GET arm.
///
/// The chain is the one kolt's server writes: `serve_build(require_build(..))`
/// (which reads the leg's manifest through `std::fs::stat` and `readFile` at
/// boot), `cache_build` with a validating policy, `on_request` for every path
/// the build does not claim, `on_start`. Three exchanges, each compared whole
/// between the legs (status, the headers the program and std set, the body):
/// the artifact with its `ETag` and `Cache-Control`; a REVALIDATION with that
/// `ETag` in `If-None-Match`, which is the `304` with no body and no length
/// (node sends none, and neither may this runtime — `Content-Length: 0` was
/// what it sent before this slice); and the fallback.
///
/// Red at the Order 40 seal: `Server::builder()` was refused by name (the
/// mutating `Bytes` bindings, `now_millis`, SHA-1), then — past those — rustc
/// refused fourteen emitted errors across five lowering classes (a `&mut` loan
/// handed on, a literal beside a `u32`, a consumed place read at a `let`, a
/// tuple, a field; an `async` hook in an `Option` field).
#[test]
fn the_builder_serves_its_build_and_revalidates_it_over_a_real_socket() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_builder.vl"), BUILDER_PROBE)
        .expect("write the probe program");
    let dist = staged.join("dist");
    std::fs::create_dir_all(&dist).expect("create the build directory");
    std::fs::write(dist.join("client.js"), "console.log(\"client\");\n")
        .expect("write the artifact");
    std::fs::write(
        dist.join("client.chunks.json"),
        "{\"leg\":\"client\",\"entry\":\"client.js\",\"styles\":null,\
         \"classic_script\":false,\"chunks\":[],\"assets\":[]}",
    )
    .expect("write the build manifest");

    let built = vilan(&staged)
        .args(["build", "--backend", "rust", "native_probe_builder.vl"])
        .output()
        .expect("build the server natively");
    assert!(
        built.status.success(),
        "the native leg did not build:\n{}{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    let binary = String::from_utf8_lossy(&built.stdout)
        .lines()
        .find_map(|line| line.split(" -> ").nth(1).map(str::to_string))
        .expect("`vilan build` says where the binary is");
    let mut native_command = Command::new(staged.join(&binary));
    native_command.current_dir(&staged);
    let native = revalidated_exchanges(native_command);

    let bundled = vilan(&staged)
        .args(["build", "native_probe_builder.vl"])
        .output()
        .expect("build the server for node");
    assert!(
        bundled.status.success(),
        "the JS leg did not build:\n{}",
        String::from_utf8_lossy(&bundled.stderr)
    );
    let mut node = Command::new("node");
    node.current_dir(&staged).arg("native_probe_builder.mjs");
    let javascript = revalidated_exchanges(node);

    assert_eq!(
        native, javascript,
        "the two backends must answer the same three exchanges"
    );
    let [artifact, revalidated, fallback] = &native[..] else {
        panic!("three exchanges, got {native:?}");
    };
    assert_eq!(artifact.status, "HTTP/1.1 200 OK");
    assert_eq!(artifact.body, "console.log(\"client\");\n");
    assert!(
        artifact
            .headers
            .iter()
            .any(|line| line.starts_with("ETag: \""))
            && artifact
                .headers
                .iter()
                .any(|line| line == "Cache-Control: no-cache"),
        "the validating policy's headers: {:?}",
        artifact.headers
    );
    assert_eq!(revalidated.status, "HTTP/1.1 304 Not Modified");
    assert_eq!(revalidated.body, "");
    assert!(
        !revalidated
            .headers
            .iter()
            .any(|line| line.starts_with("Content-Length")),
        "a 304 declares no length: {:?}",
        revalidated.headers
    );
    assert_eq!(fallback.status, "HTTP/1.1 200 OK");
    assert_eq!(fallback.body, "fallback /other");
}

/// The artifact, its revalidation with the `ETag` the first answer carried,
/// and a path the build does not claim — over one spawned server.
fn revalidated_exchanges(mut command: Command) -> Vec<ServedRequest> {
    let server = ServerUnderTest::spawn(&mut command);
    let port = server.port();
    let artifact = ServedRequest::exchange(port, "GET", "/client.js", "");
    let etag = artifact
        .headers
        .iter()
        .find_map(|line| line.strip_prefix("ETag: "))
        .unwrap_or("\"none\"")
        .to_string();
    let revalidated = ServedRequest::exchange_with(
        port,
        "GET",
        "/client.js",
        &format!("If-None-Match: {etag}\r\n"),
        "",
    );
    let fallback = ServedRequest::exchange(port, "GET", "/other", "");
    vec![artifact, revalidated, fallback]
}

const BUILDER_PROBE: &str = concat!(
    "import std::build::require_build;\n",
    "import std::http::{ CachePolicy, Response, Server };\n",
    "import std::io::print;\n",
    "\n",
    "async fun main() {\n",
    "\tlet build = require_build(\"client\");\n",
    "\tServer::builder()\n",
    "\t\t.port(0)\n",
    "\t\t.serve_build(build)\n",
    "\t\t.cache_build(|url| CachePolicy::validated().cache_control(\"no-cache\"))\n",
    "\t\t.on_request(|request| Response::builder()\n",
    "\t\t\t.set_header(\"Content-Type\", \"text/plain\")\n",
    "\t\t\t.body(i\"fallback {request.path()}\")\n",
    "\t\t\t.build())\n",
    "\t\t.on_start(|server| print(i\"vilan-test-port={server.port()}\"))\n",
    "\t\t.build()\n",
    "\t\t.start();\n",
    "}\n",
);

/// Rule 1's copy at EVERY position that consumes a place read, for the types
/// the JS backend never copies (`str`, `Option`, enums, handles): a `let`, a
/// struct literal's field, a list and a tuple element, an assignment, a `push`,
/// a subscript read out of a `Vec` at a `let` and at a tail, and a FIELD handed
/// to a call while its struct is read again. Each was a use-after-move (or a
/// move out of a `Vec`) natively — eleven rustc errors at the Order 40 seal —
/// and all of them are on `Server::builder()`'s std path.
#[test]
fn a_consumed_place_read_is_copied_at_every_consuming_position() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_consumed.vl"), CONSUMED_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_consumed.vl"),
        Verdict::Identical,
        "a place read into a consuming position is a copy on both backends"
    );
}

const CONSUMED_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::option::Option::{ self, None, Some };\n",
    "\n",
    "struct Asset {\n",
    "\tname: str,\n",
    "\tkind: Option<str>,\n",
    "}\n",
    "\n",
    "fun describe(name: str, kind: Option<str>): str {\n",
    "\tlet shown: str = kind.unwrap_or(\"-\");\n",
    "\tname + \":\" + shown\n",
    "}\n",
    "\n",
    "fun last(path: str): str {\n",
    "\tlet parts = path.split(\"/\");\n",
    "\tparts[parts.len() - 1]\n",
    "}\n",
    "\n",
    "fun kind_of(asset: Asset): Option<str> {\n",
    "\tasset.kind\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet parts = \"a/b/c\".split(\"/\");\n",
    "\tlet first = parts[0];\n",
    "\tlet copy = first;\n",
    "\tprint(i\"{first} {copy} {last(\"x/y\")} {parts[1]}\");\n",
    "\tlet asset = Asset { name = \"logo\", kind = Some(\"svg\") };\n",
    "\tprint(describe(asset.name, asset.kind));\n",
    "\tlet again = asset;\n",
    "\tlet kind: str = kind_of(asset).unwrap_or(\"?\");\n",
    "\tprint(i\"{asset.name} {again.name} {kind}\");\n",
    "\tmut kept: List<(str, i32)> = [];\n",
    "\tfor entry in [(\"x\", 1), (\"y\", 2)] {\n",
    "\t\tlet (address, at) = entry;\n",
    "\t\tkept.push((address, at));\n",
    "\t\tif address == \"y\" {\n",
    "\t\t\tprint(i\"kept {address} at {at}\");\n",
    "\t\t}\n",
    "\t}\n",
    "\tprint(kept.len());\n",
    "\tlet maybe: Option<str> = Some(\"m\");\n",
    "\tlet other = maybe;\n",
    "\tlet built = Asset { name = first, kind = maybe };\n",
    "\tlet listed = [first, copy];\n",
    "\tmut target = \"t\";\n",
    "\ttarget = first;\n",
    "\tlet shown: str = maybe.unwrap_or(\"\");\n",
    "\tlet also: str = other.unwrap_or(\"\");\n",
    "\tprint(i\"{shown} {also} {built.name} {listed.len()} {target} {first}\");\n",
    "}\n",
);

/// A binding that IS a `&mut` loan — a `&mut` parameter, a `&mut self` — is
/// REBORROWED when handed to another `&mut` position (`&mut *loan`), not
/// borrowed again: `&mut loan` is a `&mut &mut T` that rustc takes only from a
/// `mut` binding. `[derive(Wire)]`'s `describe` hands its serializer on this
/// way, which is how `Server::builder()`'s rpc error encoding reached it.
#[test]
fn a_mutable_loan_is_reborrowed_when_it_is_handed_on() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_reborrow.vl"), REBORROW_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_reborrow.vl"),
        Verdict::Identical,
        "a forwarded `&mut` loan writes the caller's value on both backends"
    );
}

const REBORROW_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "struct Counter {\n",
    "\tn: i32,\n",
    "}\n",
    "\n",
    "impl Counter {\n",
    "\tfun bump(&mut self) {\n",
    "\t\tself.n = self.n + 1;\n",
    "\t}\n",
    "\tfun twice(&mut self) {\n",
    "\t\tself.bump();\n",
    "\t\tself.bump();\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun step(counter: &mut Counter) {\n",
    "\tcounter.bump();\n",
    "}\n",
    "\n",
    "fun forward(counter: &mut Counter) {\n",
    "\tstep(counter);\n",
    "\tcounter.twice();\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tmut counter = Counter { n = 0 };\n",
    "\tforward(&mut counter);\n",
    "\tprint(counter.n);\n",
    "}\n",
);

/// An unsuffixed literal takes its PARTNER'S width across a binary operator
/// (`unit >= 65` over a `u32` is a `u32` comparison), and a host binding's
/// argument takes its own parameter's type rather than the enclosing
/// position's (`let unit: u32 = "A".code_at(0)` — the index is an `i32`).
/// `std::string`'s `to_lowercase_ascii`, on `Request::header`'s path, is the
/// first shape; seven rustc errors at the Order 40 seal.
#[test]
fn a_literal_operand_takes_its_partners_width() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_partner.vl"), PARTNER_WIDTH_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_partner.vl"),
        Verdict::Identical,
        "a literal beside a `u32` is a `u32` on both backends"
    );
}

const PARTNER_WIDTH_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun main() {\n",
    "\tlet unit: u32 = \"A\".code_at(0);\n",
    "\tlet other: u32 = 70;\n",
    "\tlet wide: u53 = 9;\n",
    "\tprint(i\"{unit >= 65 && unit <= 90} {other > 65} {unit == 65} {unit - 65}\");\n",
    // A literal on the LEFT is B389's (solver-41): the analyzer refuses it
    // before either backend sees it, so only right-hand literals are here.
    "\tprint(i\"{wide * 2 > 17} {(wide + 1) == 10}\");\n",
    "}\n",
);

/// A closure TYPE written over a VIEW (`|&mut T| void`, `|&T| U`) takes its
/// argument by reference natively, as the literal landing in it binds it —
/// the analyzer records the `&`/`&mut` the type itself erases
/// (`Program::closure_type_parameter_views`). `SignalCell::update(|&mut list|
/// ..)` is the shape everything reactive writes through (kolt's store, the
/// keyed rpc mirrors, `ListCell`), and it was refused ("an unresolved type")
/// at the Order 40 seal; a user function taking one is here too.
#[test]
fn a_closure_type_over_a_view_takes_its_argument_by_reference() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_views.vl"), VIEW_CLOSURE_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_views.vl"),
        Verdict::Identical,
        "a view-parameter closure writes the caller's value on both backends"
    );
}

const VIEW_CLOSURE_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::hash_map::HashMap;\n",
    "import std::reactive::{ Signal, SignalCell };\n",
    "\n",
    "struct Counter {\n",
    "\tn: i32,\n",
    "}\n",
    "\n",
    "fun apply(counter: &mut Counter, step: |&mut Counter| void) {\n",
    "\tstep(counter);\n",
    "\tstep(counter);\n",
    "}\n",
    "\n",
    "fun peek(counter: &Counter, read: |&Counter| i32): i32 {\n",
    "\tread(counter)\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tmut counter = Counter { n = 1 };\n",
    "\tapply(&mut counter, |&mut held| {\n",
    "\t\theld.n = held.n * 3;\n",
    "\t});\n",
    "\tprint(peek(&counter, |&held| held.n + 1));\n",
    "\tlet list: SignalCell<List<i32>> = SignalCell::new([]);\n",
    "\tlist.update(|&mut items| {\n",
    "\t\titems.push(4);\n",
    "\t\titems.push(5);\n",
    "\t});\n",
    "\tlet names: SignalCell<HashMap<str, i32>> = Signal::new(HashMap::new());\n",
    "\tnames.update(|&mut entries| {\n",
    "\t\tentries.insert(\"a\", 1);\n",
    "\t});\n",
    "\tprint(i\"{counter.n} {list.get().len()} {names.get().len()}\");\n",
    "}\n",
);

/// The one shape a closure-over-a-view cannot take natively, refused BY NAME
/// at compile time: a closure handed `update`'s `&mut` view that reads the
/// SAME cell again inside it. The JS backend answers the in-progress value (the
/// view and the cell are one object); natively the view is a live `RefMut` and
/// the read a second borrow, which safe Rust answers with a panic — so it is
/// named rather than run. `signal-update.vl`'s last section is this shape, and
/// the whole-set differential counts it refused. The control beside it reads a
/// DIFFERENT cell inside the closure and builds, identical on both backends.
#[test]
fn a_reentrant_read_of_an_updated_cell_is_refused_by_name() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_reentrant.vl"),
        concat!(
            "import std::io::print;\n",
            "import std::reactive::{ Signal, SignalCell };\n",
            "\n",
            "fun main() {\n",
            "\tlet todos: SignalCell<List<i32>> = Signal::new([1]);\n",
            "\ttodos.update(|&mut list| {\n",
            "\t\tlist.push(2);\n",
            "\t\tprint(todos.get().len());\n",
            "\t});\n",
            "}\n",
        ),
    )
    .expect("write the refused probe");
    std::fs::write(
        staged.join("native_probe_other_cell.vl"),
        concat!(
            "import std::io::print;\n",
            "import std::reactive::{ Signal, SignalCell };\n",
            "\n",
            "fun main() {\n",
            "\tlet todos: SignalCell<List<i32>> = Signal::new([1]);\n",
            "\tlet other: SignalCell<List<i32>> = Signal::new([7, 8]);\n",
            "\ttodos.update(|&mut list| {\n",
            "\t\tlist.push(other.get().len().as_i32());\n",
            "\t});\n",
            "\tprint(todos.get());\n",
            "}\n",
        ),
    )
    .expect("write the control");
    match compare(&staged, "native_probe_reentrant.vl") {
        Verdict::Refused(reason) => assert!(
            reason.contains("reads the same place again"),
            "refused, and for this reason: {reason}"
        ),
        other => panic!("a reentrant read of an updated cell must be refused by name: {other:?}"),
    }
    assert_eq!(
        compare(&staged, "native_probe_other_cell.vl"),
        Verdict::Identical,
        "reading a different cell inside the closure is not the refused shape"
    );
}

/// A statement takes no expectation from the block around it (I5 S2's hand
/// loops met this in `std::json`): a `usize` counter walked down inside an `if`
/// arm of a function answering `i32` emitted its condition's literal at the
/// function's width, `while (index > (0i32))`, and rustc refused it. Both arm
/// orders and a plain body, so the fix is not one position's.
#[test]
fn a_loop_inside_an_arm_takes_no_expectation_from_the_function() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_arm_loop.vl"), ARM_LOOP_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_arm_loop.vl"),
        Verdict::Identical,
        "a loop's literals keep their own width inside an arm"
    );
}

const ARM_LOOP_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun down(xs: List<i32>, flag: bool): i32 {\n",
    "\tif flag {\n",
    "\t\tmut index = xs.len();\n",
    "\t\tfor index > 0 {\n",
    "\t\t\tindex -= 1;\n",
    "\t\t\tprint(xs[index]);\n",
    "\t\t}\n",
    "\t\txs.len().as_i32()\n",
    "\t} else {\n",
    "\t\t0\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun up(xs: List<i32>, flag: bool): u53 {\n",
    "\tif !flag {\n",
    "\t\t0\n",
    "\t} else {\n",
    "\t\tmut at = 0;\n",
    "\t\tfor at < xs.len() {\n",
    "\t\t\tprint(xs[at]);\n",
    "\t\t\tat += 1;\n",
    "\t\t}\n",
    "\t\tat.as_u53()\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun count(xs: List<i32>): i32 {\n",
    "\tmut left = xs.len();\n",
    "\tfor left > 0 {\n",
    "\t\tleft -= 1;\n",
    "\t}\n",
    "\t7\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tprint(down([1, 2, 3], true));\n",
    "\tprint(up([4, 5], true));\n",
    "\tprint(count([6]));\n",
    "}\n",
);

/// A generic call whose binding the call itself leaves OPEN is closed by the
/// position it fills (B370's law on the generic-call path): `SignalCell::new([])`
/// under a `SignalCell<List<i32>>` annotation, `Signal::new(Map::new())` whose
/// argument's own binding is closed by the parameter the outer call binds, and a
/// variant whose payload leaves a parameter open (`Delta::Remove("k")`,
/// `Delta::Reset([1, 2])`) pushed into a `List<Delta<str, i32>>`. Refused at
/// the Order 40 seal ("an unresolved type").
#[test]
fn a_generic_call_left_open_is_closed_by_its_position() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_open.vl"), OPEN_BINDING_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_open.vl"),
        Verdict::Identical,
        "an open binding closed by its position builds the same value on both backends"
    );
}

const OPEN_BINDING_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::hash_map::HashMap;\n",
    "import std::reactive::{ Signal, SignalCell };\n",
    "\n",
    "enum Delta<K, T> {\n",
    "\tReset(List<T>),\n",
    "\tRemove(K),\n",
    "}\n",
    "\n",
    "fun count_resets(ops: List<Delta<str, i32>>): usize {\n",
    "\tmut resets = 0;\n",
    "\tfor op in ops {\n",
    "\t\tmatch op {\n",
    "\t\t\tDelta::Reset(let items) => resets += items.len(),\n",
    "\t\t\tDelta::Remove(let _key) => {},\n",
    "\t\t}\n",
    "\t}\n",
    "\tresets\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet cell: SignalCell<List<i32>> = SignalCell::new([]);\n",
    "\tlet named: SignalCell<HashMap<str, i32>> = Signal::new(HashMap::new());\n",
    "\tmut ops: List<Delta<str, i32>> = [];\n",
    "\tops.push(Delta::Reset([1, 2]));\n",
    "\tops.push(Delta::Remove(\"k\"));\n",
    "\tprint(i\"{cell.get().len()} {named.get().len()} {ops.len()} {count_resets(ops)}\");\n",
    "}\n",
);

/// Four lowering gaps kolt's server shape reached, each general: a `const`
/// expression's COMPUTED value in place of its subtree (`const
/// asset::read(..)`, as the JS emitter serializes it); a `str` literal NESTED in
/// a `match` pattern (`Some("api")` over an `Option<str>`, now a binding and a
/// guard); an unannotated function's return type from the analyzer's own
/// inference (`is_fingerprinted`'s bare `true` tail had emitted `-> ()`); and a
/// NESTED closure's destructured parameters kept out of the outer closure's
/// captures (`|(k, _)| k == key` inside `get_post_var`).
#[test]
fn the_kolt_shapes_lowering_gaps_build_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_lowering.vl"), KOLT_LOWERING_PROBE)
        .expect("write the probe program");
    std::fs::write(staged.join("native-head.txt"), "baked at build time")
        .expect("write the asset the probe reads at build time");
    assert_eq!(
        compare(&staged, "native_probe_lowering.vl"),
        Verdict::Identical,
        "the four shapes print the same on both backends"
    );
}

const KOLT_LOWERING_PROBE: &str = concat!(
    "import std::asset;\n",
    "import std::io::print;\n",
    "import std::option::Option::{ self, None, Some };\n",
    "\n",
    "fun is_short(text: str) {\n",
    "\tif text.len() > 3 {\n",
    "\t\tret false;\n",
    "\t}\n",
    "\ttrue\n",
    "}\n",
    "\n",
    "fun route(parts: List<str>): str {\n",
    "\tmatch parts.get(0) {\n",
    "\t\tSome(\"api\") => match parts.get(1) {\n",
    "\t\t\tSome(\"login\") => \"login\",\n",
    "\t\t\tSome(let other) => \"api:\" + other,\n",
    "\t\t\tNone => \"api\",\n",
    "\t\t},\n",
    "\t\tSome(let first) => \"page:\" + first,\n",
    "\t\tNone => \"root\",\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet pairs = [(\"a\", 1), (\"b\", 2)];\n",
    "\tlet find = |key: str| pairs.find(|(k, _)| k == key).map(|(_, v)| v);\n",
    "\tprint(i\"{is_short(\"ab\")} {is_short(\"abcd\")}\");\n",
    "\tprint(route([\"api\", \"login\"]));\n",
    "\tprint(route([\"api\", \"x\"]));\n",
    "\tprint(route([\"home\"]));\n",
    "\tprint(route([]));\n",
    "\tprint(find(\"b\").unwrap_or(0));\n",
    "\tprint(const asset::read(\"native-head.txt\"));\n",
    "}\n",
);

/// **F34**: the two reactive COMBINATORS build natively. `map<U>` is a trait
/// DEFAULT with a generic parameter of its own, reached through a dispatch the
/// analyzer records no value for — `U` is bound from the closure argument
/// (`|n| n * 10` against `|T| U`), and the default is one instance per binding
/// of it as well as per receiver. `flatten` is written with `?` lifts
/// (`inner_subscription.read()?.dispose()`), which lower to a `match` that
/// rebuilds the bad half. Both refused at the Order 40 seal (`parameter 1 of
/// map`; `a ? lift`); `reactive.vl`, `reactive-on-change.vl`,
/// `reactive-flatten.vl` and `iterator-adapters.vl` flip with them, and two of
/// those are in [`DEFAULT_SUITE`]. A142 S1 renamed `map` to `derive` and made
/// the total join `switch(|inner| inner)`; the probes seal what they read with
/// `.memo()`, since a pipe has no `get`.
#[test]
fn the_reactive_combinators_map_and_flatten_build_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_map.vl"), MAP_PROBE).expect("write the map probe");
    std::fs::write(staged.join("native_probe_flatten.vl"), FLATTEN_PROBE)
        .expect("write the flatten probe");
    for program in ["native_probe_map.vl", "native_probe_flatten.vl"] {
        assert_eq!(
            compare(&staged, program),
            Verdict::Identical,
            "{program}: a derived signal must print the same on both backends"
        );
    }
}

/// **F34 on A124 S2b's nodes**: a `.cell()` chain (`map` into a cached cell,
/// mapped again and cached again) and a `.distinct()` node that passes a change
/// on only when the value differs — its subscriber counts the changes that got
/// through — build natively and print the same as node.
#[test]
fn a_cell_chain_and_a_distinct_node_build_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_cell.vl"), CELL_CHAIN_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_cell.vl"),
        Verdict::Identical,
        "a `.cell()` chain and a `.distinct()` must print the same on both backends"
    );
}

const CELL_CHAIN_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::reactive::{ Signal, SignalCell, Source };\n",
    "\n",
    "fun main() {\n",
    "\tlet count = SignalCell::new(1);\n",
    "\tlet scaled = count.derive(|n| n * 10).memo();\n",
    "\tlet labelled = scaled.derive(|n| i\"#{n}\").memo();\n",
    "\tlet parity = count.derive(|n| n % 2).distinct().memo();\n",
    "\tmut changes = 0;\n",
    "\tlet _watch = parity.sub(|value| {\n",
    "\t\tchanges += 1;\n",
    "\t});\n",
    "\tcount.set(3);\n",
    "\tcount.set(4);\n",
    "\tcount.set(6);\n",
    "\tlet now: i32 = scaled.get();\n",
    "\tlet label: str = labelled.get();\n",
    "\tlet odd: i32 = parity.get();\n",
    "\tprint(i\"{now} {label} {odd} {changes}\");\n",
    "}\n",
);

const MAP_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::reactive::{ Signal, SignalCell, Source };\n",
    "\n",
    "fun main() {\n",
    "\tlet count = SignalCell::new(1);\n",
    "\tlet scaled = count.derive(|n| n * 10).memo();\n",
    "\tlet labelled = scaled.derive(|n| i\"#{n}\").memo();\n",
    "\tlet halves = count.derive(|n| n.as_f64() / 2.0).memo();\n",
    "\tcount.set(4);\n",
    "\tlet now: i32 = scaled.get();\n",
    "\tlet label: str = labelled.get();\n",
    "\tlet half: f64 = halves.get();\n",
    "\tprint(i\"{now} {label} {half}\");\n",
    "\tcount.set(5);\n",
    "\tlet later: i32 = scaled.get();\n",
    "\tprint(later);\n",
    "}\n",
);

const FLATTEN_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::reactive::{ Signal, SignalCell, Source };\n",
    "\n",
    "fun main() {\n",
    "\tlet first = SignalCell::new(1);\n",
    "\tlet second = SignalCell::new(2);\n",
    "\tlet chosen = SignalCell::new(first);\n",
    "\tlet joined = chosen.switch(|inner: SignalCell<i32>| inner).memo();\n",
    "\tfirst.set(10);\n",
    "\tchosen.set(second);\n",
    "\tsecond.set(20);\n",
    "\tprint(joined.get());\n",
    "}\n",
);

/// Builds `program` (already staged) natively and for node, and answers the
/// two commands that START each leg's server: the native binary and `node
/// <program>.mjs`, both run from the staging directory so a relative `dist/`
/// and a `const asset::read` beside the program resolve the same way.
fn both_servers(staged: &Path, program: &str) -> (Command, Command) {
    let built = vilan(staged)
        .args(["build", "--backend", "rust", program])
        .output()
        .expect("build the server natively");
    assert!(
        built.status.success(),
        "the native leg did not build:\n{}{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    let binary = String::from_utf8_lossy(&built.stdout)
        .lines()
        .find_map(|line| line.split(" -> ").nth(1).map(str::to_string))
        .expect("`vilan build` says where the binary is");
    let bundled = vilan(staged)
        .args(["build", program])
        .output()
        .expect("build the server for node");
    assert!(
        bundled.status.success(),
        "the JS leg did not build:\n{}",
        String::from_utf8_lossy(&bundled.stderr)
    );
    let mut native = Command::new(staged.join(&binary));
    native.current_dir(staged);
    let mut node = Command::new("node");
    node.current_dir(staged).arg(program.replace(".vl", ".mjs"));
    (native, node)
}

/// Builds a client program for node (clients are always the JS leg: the
/// generated rpc client is a browser-shaped program).
fn build_client(staged: &Path, program: &str) {
    let bundled = vilan(staged)
        .args(["build", program])
        .output()
        .expect("build the client for node");
    assert!(
        bundled.status.success(),
        "the client did not build:\n{}",
        String::from_utf8_lossy(&bundled.stderr)
    );
}

/// Runs a node client to completion under a LIVENESS bound (the clients exit
/// by themselves; the bound only turns a hang into a failure), and answers its
/// stdout.
fn run_client(staged: &Path, program: &str, environment: &[(&str, String)]) -> String {
    let mut command = Command::new("node");
    command
        .current_dir(staged)
        .arg(program.replace(".vl", ".mjs"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit());
    for (name, value) in environment {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("spawn the client");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        if child.try_wait().expect("poll the client").is_some() {
            break;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            panic!("the client never finished");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let mut out = String::new();
    child
        .stdout
        .take()
        .expect("the client's stdout")
        .read_to_string(&mut out)
        .expect("read the client's stdout");
    out
}

/// **F18 slice 3 — the rpc server natively.** `service_layer.rs`'s keyed pin
/// (A39) with its two halves pulled apart: the SERVER — a `[service]` with an
/// `[expose]` and an `[expose(keyed)]` field, mounted with `Service::new` on
/// `Server::builder()` — is built by each backend in turn, and the CLIENT,
/// always node, connects over the upgrade handover (a real RFC 6455 accept key,
/// computed by `vilan_rt::crypto`), holds a PER-KEY subscription on the keyed
/// mirror, posts, edits, and prints what its mirrors hold and the contract hash
/// it computed.
///
/// The client's whole stdout is compared between the two servers, and the
/// server's own announced contract hash is compared with the client's — the
/// wire is byte-identical to node's exactly when a vilan client cannot tell
/// the two servers apart, and the hash is unmoved exactly when the native
/// server hashes its contract as node does (a moved hash is a client refused
/// as `Contract`, which the verbatim lines below would red on).
///
/// Red at the Order 40 seal: `Server::builder()` refused by name; past it, the
/// rpc path reached `SignalCell::update(|&mut store| ..)` (a closure TYPE's view
/// parameter, which the type erased), `Signal::new(Map::new())` (a binding the
/// position closes), `keyed_diff`'s `ops.push(Delta::Reset(..))` (an open
/// variant parameter), a `self` captured into a stored closure, and a context
/// argument the dispatched `SignalCell::sub` does not take.
#[test]
fn the_keyed_rpc_service_answers_a_node_client_the_same_from_a_native_server() {
    let staged = stage();
    std::fs::write(
        staged.join("native_keyed_server.vl"),
        include_str!("native/keyed_chat_server.vl"),
    )
    .expect("write the server");
    std::fs::write(
        staged.join("native_keyed_client.vl"),
        include_str!("native/keyed_chat_client.vl"),
    )
    .expect("write the client");
    build_client(&staged, "native_keyed_client.vl");
    let (mut native_command, mut node_command) = both_servers(&staged, "native_keyed_server.vl");
    let serve = |command: &mut Command| {
        let server = ServerUnderTest::spawn(command);
        let answered = run_client(
            &staged,
            "native_keyed_client.vl",
            &[("CHAT_PORT", server.port().to_string())],
        );
        let contract = server
            .announcement
            .split_whitespace()
            .find_map(|field| field.strip_prefix("contract="))
            .unwrap_or_default()
            .to_string();
        (answered, contract)
    };
    let (native, native_contract) = serve(&mut native_command);
    let (javascript, node_contract) = serve(&mut node_command);
    assert_eq!(
        native, javascript,
        "a node client must see the same wire from both servers"
    );
    assert_eq!(
        native_contract, node_contract,
        "the contract hash is unmoved"
    );
    for line in [
        "post:2",
        "edit:true",
        "m2:world again",
        "held:m2=world again ",
        "topic-held:general",
        "fault:false",
    ] {
        assert!(
            native.lines().any(|answered| answered == line),
            "the client's `{line}` over the native server:\n{native}"
        );
    }
    assert!(
        native
            .lines()
            .any(|line| line == format!("hash:{native_contract}")),
        "the client's contract hash is the server's own ({native_contract}):\n{native}"
    );
}

/// **F18 slice 3's EXIT** — a program with the SHAPE of kolt's server leg,
/// `Server::builder()` as kolt writes it, built natively: an auth door
/// (`/api/register`, `/api/login`) decoding kolt's `List<List<str>>` POST body
/// through `parse_path` as kolt writes it and checking a hashed password in
/// SQLite; a per-connection rpc store (`Service::factory`) behind a handshake
/// gate (`authorize`) that looks the session token up in the same database; the
/// build served from its own description with kolt's fingerprint-aware
/// `cache_build`; and the `Document` shell (`const asset::read` in its head) for
/// every other path. The program lives in `native/kolt_shape_server.vl` and is
/// written from scratch — kolt is never copied into this tree.
///
/// Over ONE server: six HTTP exchanges, each byte for byte against node's
/// (status line, the headers the program and std set, the body): a register, a
/// good login (whose answer carries the session token), a wrong password, a
/// malformed call, a deep link answered by the shell, and the client bundle
/// with its validator. Then two node clients over the WebSocket upgrade: one
/// with the token — `whoami` is the identity the handshake settled, a PER-KEY
/// subscription sees its key's writes and no other — and one with a bogus
/// token, refused at the handshake. Both clients' stdout compared whole.
///
/// Non-vacuous by content: the bodies and the client's lines are asserted
/// verbatim, and the two logins differ only in the password.
///
/// **F40: the REAL password path.** The shape hashes as kolt's `store.vl` does —
/// PBKDF2-HMAC-SHA-512 at 100,000 rounds through a node:crypto `pbkdf2Sync` the
/// program binds itself, read back through the `Buffer`'s `toString("hex")`, a
/// salt and a session token from `std::crypto::random_bytes` — so both legs run
/// `vilan-rt-crypto`'s arithmetic against node's. The tokens are random by
/// design and are compared MASKED ([`mask_session_tokens`]); the authorized
/// client carries the real one, so a token the store did not keep is a refused
/// client. PBKDF2's bytes themselves are held to node's in
/// [`the_crypto_surface_answers_nodes_bytes_on_both_backends`]; kolt's own
/// `server.vl`, built natively on a scratch copy, is measured beside the
/// order's census (`sweeps/order42/native-42/`), where one backend's hash is
/// verified by the other over one database file.
#[test]
fn the_kolt_server_shape_serves_a_login_and_a_keyed_subscription_from_a_native_build() {
    let staged = stage();
    for (name, contents) in [
        (
            "native_kolt_server.vl",
            include_str!("native/kolt_shape_server.vl"),
        ),
        (
            "native_kolt_client.vl",
            include_str!("native/kolt_shape_client.vl"),
        ),
        ("server-head.html", include_str!("native/server-head.html")),
    ] {
        std::fs::write(staged.join(name), contents).expect("stage the exit program");
    }
    let dist = staged.join("dist");
    std::fs::create_dir_all(&dist).expect("create the build directory");
    std::fs::write(dist.join("client.js"), "console.log(\"client\");\n")
        .expect("write the artifact");
    std::fs::write(
        dist.join("client.chunks.json"),
        "{\"leg\":\"client\",\"entry\":\"client.js\",\"styles\":null,\
         \"classic_script\":false,\"chunks\":[],\"assets\":[]}",
    )
    .expect("write the build manifest");
    build_client(&staged, "native_kolt_client.vl");
    let (mut native_command, mut node_command) = both_servers(&staged, "native_kolt_server.vl");

    let serve = |command: &mut Command| {
        let server = ServerUnderTest::spawn(command);
        let port = server.port();
        let credentials = "[[\"username\",\"ada\"],[\"password\",\"lovelace1\"]]";
        let exchanges = vec![
            ServedRequest::exchange(port, "POST", "/api/register", credentials),
            ServedRequest::exchange(port, "POST", "/api/login", credentials),
            ServedRequest::exchange(
                port,
                "POST",
                "/api/login",
                "[[\"username\",\"ada\"],[\"password\",\"wrong-one\"]]",
            ),
            ServedRequest::exchange(port, "POST", "/api/login", "[[\"username\",\"ada\"]]"),
            ServedRequest::exchange(port, "GET", "/some/deep/link", ""),
            ServedRequest::exchange(port, "GET", "/client.js", ""),
        ];
        let token = exchanges[1]
            .body
            .split("\"token\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap_or_default()
            .to_string();
        let authorized = run_client(
            &staged,
            "native_kolt_client.vl",
            &[("KOLT_PORT", port.to_string()), ("KOLT_TOKEN", token)],
        );
        let refused = run_client(
            &staged,
            "native_kolt_client.vl",
            &[
                ("KOLT_PORT", port.to_string()),
                ("KOLT_TOKEN", "bogus".to_string()),
            ],
        );
        (exchanges, authorized, refused)
    };
    let mut native = serve(&mut native_command);
    let mut javascript = serve(&mut node_command);
    // F40: the salt and the session token are `random_bytes(32).to_hex()`
    // on both legs, as in kolt, so each body is compared with its tokens
    // MASKED — and the tokens themselves are held to what random ones must
    // be: 64 lowercase hex digits, fresh per session, never the other leg's.
    let native_tokens = mask_session_tokens(&mut native.0);
    let node_tokens = mask_session_tokens(&mut javascript.0);
    assert_eq!(
        native.0, javascript.0,
        "the six HTTP exchanges must be the same from both servers"
    );
    for tokens in [&native_tokens, &node_tokens] {
        assert_eq!(tokens.len(), 2, "register and login each open a session");
        assert_ne!(tokens[0], tokens[1], "a session token is fresh per session");
    }
    assert!(
        native_tokens
            .iter()
            .all(|token| !node_tokens.contains(token)),
        "two processes never draw the same token: {native_tokens:?} {node_tokens:?}"
    );
    assert_eq!(
        native.1, javascript.1,
        "the authorized client must see the same wire from both servers"
    );
    assert_eq!(
        native.2, javascript.2,
        "the refused client must be refused the same way by both servers"
    );

    let bodies: Vec<&str> = native
        .0
        .iter()
        .map(|exchange| exchange.body.as_str())
        .collect();
    assert_eq!(
        bodies[..4],
        [
            "{\"ok\":true,\"token\":\"<session token>\",\"message\":\"welcome ada\"}",
            "{\"ok\":true,\"token\":\"<session token>\",\"message\":\"welcome ada\"}",
            "{\"ok\":false,\"token\":\"\",\"message\":\"wrong password\"}",
            "malformed call",
        ]
    );
    assert_eq!(native.0[3].status, "HTTP/1.1 400 Bad Request");
    assert!(
        bodies[4].contains("<title>Kolt</title>")
            && bodies[4].contains("<meta name=\"shape\" content=\"kolt\">"),
        "the shell, with the head `const asset::read` baked in:\n{}",
        bodies[4]
    );
    assert_eq!(bodies[5], "console.log(\"client\");\n");
    assert!(
        native.0[5]
            .headers
            .iter()
            .any(|line| line == "Cache-Control: no-cache"),
        "an unfingerprinted artifact is validated, not immutable: {:?}",
        native.0[5].headers
    );
    for line in [
        "who:ada",
        "post:1",
        "m2:ada:world",
        "m2:ada:world again",
        "fault:false",
    ] {
        assert!(
            native.1.lines().any(|answered| answered == line),
            "the authorized client's `{line}` over the native server:\n{}",
            native.1
        );
    }
    assert!(
        !native.1.contains("m1"),
        "a per-key subscription sees ITS key and no other:\n{}",
        native.1
    );
    assert_eq!(native.2.trim(), "err:Unauthorized");
}

/// Replaces every session token in `exchanges`' bodies — a run of exactly 64
/// lowercase hex digits, which is `random_bytes(32).to_hex()` — with a
/// placeholder, and answers the tokens in order. Anything that LOOKS like a
/// token but is not 64 lowercase hex stays in the body and fails the compare.
fn mask_session_tokens(exchanges: &mut [ServedRequest]) -> Vec<String> {
    let mut tokens = Vec::new();
    for exchange in exchanges {
        let mut masked = String::new();
        let mut rest = exchange.body.as_str();
        while let Some(start) = rest.find("\"token\":\"") {
            let (before, after) = rest.split_at(start + "\"token\":\"".len());
            masked.push_str(before);
            let end = after.find('"').unwrap_or(after.len());
            let token = &after[..end];
            if token.len() == 64
                && token
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
            {
                tokens.push(token.to_string());
                masked.push_str("<session token>");
            } else {
                masked.push_str(token);
            }
            rest = &after[end..];
        }
        masked.push_str(rest);
        exchange.body = masked;
    }
    tokens
}

/// The three exchanges the exit drives, over one spawned server.
struct ServedLogin {
    exchanges: Vec<ServedRequest>,
}

impl ServedLogin {
    fn take(mut command: Command) -> ServedLogin {
        let server = ServerUnderTest::spawn(&mut command);
        let port = server.port();
        let exchanges = [
            ("POST", "/api/login", "[\"ada\",\"lovelace1\"]"),
            ("POST", "/api/login", "[\"ada\",\"wrong\"]"),
            ("GET", "/", ""),
        ]
        .into_iter()
        .map(|(method, path, body)| ServedRequest::exchange(port, method, path, body))
        .collect();
        ServedLogin { exchanges }
    }
}

/// One request answered by a spawned server, and the pieces of the answer the
/// two backends can be held to.
#[derive(Debug, PartialEq, Eq)]
struct ServedRequest {
    status: String,
    headers: Vec<String>,
    body: String,
    announced_line: String,
}

impl ServedRequest {
    /// One request to an ALREADY-RUNNING server, so a test can drive several
    /// over one process. The body carries a `Content-Length`, which is the
    /// only framing `vilan_rt::http` accepts on the way in (a chunked request
    /// is refused with `411`, by design).
    ///
    /// `announced_line` is empty here: it belongs to the server, and a caller
    /// driving several exchanges has it from the spawn.
    fn exchange(port: u16, method: &str, path: &str, body: &str) -> ServedRequest {
        ServedRequest::exchange_with(port, method, path, "", body)
    }

    /// [`ServedRequest::exchange`] with extra request header lines (each ending
    /// `\r\n`) — how a revalidation sends its `If-None-Match`.
    fn exchange_with(
        port: u16,
        method: &str,
        path: &str,
        extra_headers: &str,
        body: &str,
    ) -> ServedRequest {
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port))
            .expect("connect to the port the server announced");
        stream
            .write_all(
                format!(
                    "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\
                     {extra_headers}Connection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .expect("send the request");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("read the response");
        let (head, body) = response
            .split_once("\r\n\r\n")
            .unwrap_or_else(|| panic!("a response with a head and a body, got {response:?}"));
        let mut lines = head.split("\r\n");
        let status = lines.next().unwrap_or_default().to_string();
        ServedRequest {
            status,
            // node adds a `Date` of its own and the two backends order
            // `Connection`/`Content-Length` differently; both are dropped here
            // so the two legs can be compared whole. See the exit's own
            // comment for why that is written down rather than normalised
            // away silently.
            headers: lines
                .filter(|line| !line.starts_with("Date:") && !line.starts_with("Connection:"))
                .map(str::to_string)
                .collect(),
            body: body.to_string(),
            announced_line: String::new(),
        }
    }

    /// Spawns `command`, waits for the port IT bound, fetches `GET /`, and
    /// reaps the child.
    ///
    /// The child is killed on the way out of this function on every path,
    /// including a panic inside it, because [`ServerUnderTest`] owns it and its
    /// `Drop` does the kill — a failed assertion must not leak a listener into
    /// the rest of the suite.
    fn take(mut command: Command) -> ServedRequest {
        let server = ServerUnderTest::spawn(&mut command);
        let port = server.port();
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port))
            .expect("connect to the port the server announced");
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .expect("send the request");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("read the response");
        let (head, body) = response
            .split_once("\r\n\r\n")
            .unwrap_or_else(|| panic!("a response with a head and a body, got {response:?}"));
        let mut lines = head.split("\r\n");
        let status = lines.next().unwrap_or_default().to_string();
        ServedRequest {
            status,
            headers: lines.map(str::to_string).collect(),
            body: body.to_string(),
            announced_line: server.announcement.clone(),
        }
    }
}

/// A spawned server whose port is the one it actually bound, killed on drop.
///
/// `support/port.rs` is the same mechanism for the e2e suites; this binary has
/// no `mod support`, and the twenty lines are cheaper than giving it one for a
/// single test.
struct ServerUnderTest {
    child: std::process::Child,
    announcement: String,
    port: u16,
}

impl ServerUnderTest {
    fn spawn(command: &mut Command) -> ServerUnderTest {
        let mut child = command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .expect("spawn the server");
        let stdout = child.stdout.take().expect("the server's stdout");
        let (sender, lines) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout)
                .lines()
                .map_while(Result::ok)
            {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let mut server = ServerUnderTest {
            child,
            announcement: String::new(),
            port: 0,
        };
        // A LIVENESS bound, not a claim about how fast a server boots: a green
        // spawn returns the moment the line lands.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            assert!(!remaining.is_zero(), "the server never announced its port");
            match lines.recv_timeout(remaining) {
                Ok(line) => {
                    if let Some(number) = line
                        .split_whitespace()
                        .find_map(|field| field.strip_prefix("vilan-test-port="))
                    {
                        let port: u16 = number.parse().expect("the announced port is a number");
                        assert_ne!(
                            port, 0,
                            "the server reported the port it ASKED for, not one it bound"
                        );
                        server.announcement = line;
                        server.port = port;
                        return server;
                    }
                }
                Err(_) => panic!("the server's stdout ended before it announced a port"),
            }
        }
    }

    fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for ServerUnderTest {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// F45: `Server::stop()` ends a native server program as it ends a node one —
/// the listener closes, `on_stop` fires, the loop runs out of work, and the
/// process exits 0 — and `on_start` runs AFTER the turn that called
/// `start()`, as node's `'listening'` does (it ran inside `start()`
/// natively, so `print("main returned")` came second; found building this).
#[test]
fn a_stopped_server_ends_the_program_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_stop.vl"), STOP_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_stop.vl"),
        Verdict::Identical,
        "a server that stops itself must end the program, printing the same lines in the same \
         order on both backends"
    );
}

const STOP_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::http::{ Server, Response };\n",
    "import std::option::Option::None;\n",
    "\n",
    "fun main() {\n",
    "\tlet server = Server {\n",
    "\t\tport = 0,\n",
    "\t\trequest_handler = |request| Response::builder().body(\"hello\\n\").build(),\n",
    "\t\ton_start = |started| {\n",
    "\t\t\tprint(\"started\");\n",
    "\t\t\tstarted.stop();\n",
    "\t\t},\n",
    "\t\ton_stop = |stopped| print(\"stopped\"),\n",
    "\t\tupgrade_handler = None,\n",
    "\t\tnode = None,\n",
    "\t};\n",
    "\tserver.start();\n",
    "\tprint(\"main returned\");\n",
    "}\n",
);

/// A native server built from `program` and spawned with `environment`: the
/// child and the port it announced.
#[cfg(unix)]
struct SignalledServer {
    child: std::process::Child,
    port: u16,
}

#[cfg(unix)]
impl SignalledServer {
    fn spawn(staged: &Path, program: &str, environment: &[(&str, &str)]) -> SignalledServer {
        let built = vilan(staged)
            .args(["build", "--backend", "rust", program])
            .output()
            .expect("build the server natively");
        assert!(
            built.status.success(),
            "the native leg did not build:\n{}{}",
            String::from_utf8_lossy(&built.stdout),
            String::from_utf8_lossy(&built.stderr)
        );
        // The graceful stop is linked only by a program that serves.
        let manifest = std::fs::read_to_string(
            staged
                .join("dist")
                .join("native")
                .join(program.trim_end_matches(".vl"))
                .join("Cargo.toml"),
        )
        .expect("read the generated manifest");
        assert!(
            manifest.contains("vilan-rt-signal"),
            "a program that starts a server links the signal crate:\n{manifest}"
        );
        let binary = String::from_utf8_lossy(&built.stdout)
            .lines()
            .find_map(|line| line.split(" -> ").nth(1).map(str::to_string))
            .expect("`vilan build` says where the binary is");
        let mut command = Command::new(staged.join(&binary));
        command
            .current_dir(staged)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        for (name, value) in environment {
            command.env(name, value);
        }
        let mut child = command.spawn().expect("spawn the server");
        let stdout = child.stdout.take().expect("the server's stdout");
        let mut reader = std::io::BufReader::new(stdout);
        let mut line = String::new();
        let port = loop {
            line.clear();
            let read = reader
                .read_line(&mut line)
                .expect("read the server's stdout");
            assert!(
                read > 0,
                "the server's stdout ended before it announced a port"
            );
            if let Some(number) = line.trim().strip_prefix("vilan-test-port=") {
                break number.parse().expect("the announced port is a number");
            }
        };
        SignalledServer { child, port }
    }

    fn signal(&self, name: &str) {
        let sent = Command::new("kill")
            .args([name, &self.child.id().to_string()])
            .status()
            .expect("run kill");
        assert!(sent.success(), "kill {name} must succeed");
    }

    /// Waits for the process to end — a LIVENESS bound, not a claim about how
    /// fast it stops — and answers its exit status and its stderr.
    fn finish(mut self) -> (std::process::ExitStatus, String) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let status = loop {
            if let Some(status) = self.child.try_wait().expect("poll the server") {
                break status;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the server did not end after the signal"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        let mut stderr = String::new();
        if let Some(mut pipe) = self.child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        (status, stderr)
    }
}

#[cfg(unix)]
impl Drop for SignalledServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// F45 (Order 43's R-i, RULED: build): a native server STOPS on SIGTERM and
/// reaches its process end, so the leak census reads a server's cells at exit.
///
/// The kolt shape answers a login, takes a SIGTERM, and exits 0 — where node,
/// with no handler, dies of the signal — printing the census line the runtime
/// prints only after the program's thread has ended. A `#[cfg(unix)]` pin: the
/// signal is sent with `kill`, and Windows has no console equivalent a test
/// could send.
#[cfg(unix)]
#[test]
fn a_native_server_stops_on_sigterm_and_reaches_its_process_end() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_kolt_exit.vl"), KOLT_SHAPE_PROBE)
        .expect("write the probe program");
    let server = SignalledServer::spawn(
        &staged,
        "native_probe_kolt_exit.vl",
        &[("VILAN_NATIVE_LEAK_CENSUS", "1")],
    );
    let login =
        ServedRequest::exchange(server.port, "POST", "/api/login", "[\"ada\",\"lovelace1\"]");
    assert_eq!(login.body, "{\"ok\":true,\"message\":\"welcome ada\"}");
    server.signal("-TERM");
    let (status, stderr) = server.finish();
    assert_eq!(
        status.code(),
        Some(0),
        "a SIGTERM'd native server drains and exits 0 (stderr: {stderr})"
    );
    let census = stderr
        .lines()
        .find(|line| line.starts_with("vilan-native: cells minted="))
        .unwrap_or_else(|| panic!("the program reached no process end:\n{stderr}"));
    assert_eq!(
        census, KOLT_SHAPE_EXIT_CENSUS,
        "the kolt shape's exit census moved; a live cell is a cycle — read it before moving \
         this line"
    );
}

/// What the kolt shape's counted cells are at process end after one login and
/// a SIGTERM — F45's exit row, C14's gate reading a server for the first time.
#[cfg(unix)]
const KOLT_SHAPE_EXIT_CENSUS: &str = "vilan-native: cells minted=1 live=0";

/// F45: the SECOND termination signal ends the process at once — the answer
/// for a server whose open response never ends. The first stops the listener
/// (a new connection is refused, which is how the harness knows it landed —
/// no sleep stands in for it) while the open stream keeps the program alive;
/// the second exits 1 with the runtime's sentence.
#[cfg(unix)]
#[test]
fn a_second_termination_signal_ends_a_server_whose_stream_never_closes() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_open_stream.vl"),
        OPEN_STREAM_PROBE,
    )
    .expect("write the probe program");
    let mut server = SignalledServer::spawn(&staged, "native_probe_open_stream.vl", &[]);
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", server.port))
        .expect("connect to the announced port");
    stream
        .write_all(b"GET /events HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .expect("send the request");
    let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone the stream"));
    let mut line = String::new();
    loop {
        line.clear();
        assert!(
            reader.read_line(&mut line).expect("read the stream") > 0,
            "the stream ended before its first chunk"
        );
        if line.contains("first") {
            break;
        }
    }
    server.signal("-TERM");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while std::net::TcpStream::connect(("127.0.0.1", server.port)).is_ok() {
        assert!(
            std::time::Instant::now() < deadline,
            "the first SIGTERM never closed the listener"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        server.child.try_wait().expect("poll the server").is_none(),
        "the open stream keeps the program alive after the first signal"
    );
    server.signal("-TERM");
    let (status, stderr) = server.finish();
    assert_eq!(
        status.code(),
        Some(1),
        "the second signal exits 1 (stderr: {stderr})"
    );
    assert!(
        stderr.contains("stopped by a second termination request"),
        "the runtime says why it stopped: {stderr}"
    );
    drop(stream);
}

#[cfg(unix)]
const OPEN_STREAM_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::http::{ Server, Response };\n",
    "import std::option::Option::None;\n",
    "\n",
    "fun main() {\n",
    "\tlet server = Server {\n",
    "\t\tport = 0,\n",
    "\t\trequest_handler = |request| Response::builder()\n",
    "\t\t\t.streaming(|stream| stream.send(\"first\\n\"))\n",
    "\t\t\t.build(),\n",
    "\t\ton_start = |started| print(i\"vilan-test-port={started.port()}\"),\n",
    "\t\ton_stop = |stopped| {},\n",
    "\t\tupgrade_handler = None,\n",
    "\t\tnode = None,\n",
    "\t};\n",
    "\tserver.start();\n",
    "}\n",
);

/// F25: a program that FAILS answers the same exit code on both backends, and
/// the native binary does not print Rust's panic banner.
///
/// Node prints the error and exits 1; Rust prints
/// `thread 'main' panicked at src/main.rs:N:M:`, a `note: run with
/// RUST_BACKTRACE=1` line, and exits 101. stdout is what the differential
/// compares and it was already identical, so this is about what a SHELL sees —
/// and a shell reading 101 where the JS build gave it 1 is one program
/// answering two different things.
///
/// Both the synchronous and the `async fun main` paths, because they are two
/// different emitted shapes: one wraps the body, the other wraps the
/// `block_on`.
/// **F32 (RULED (b), Order 40)**: `BigInt` is an `i128` natively, and the limit
/// is enforced at BOTH ends.
///
/// The two corpus programs that hold the inside of the range are in
/// [`DEFAULT_SUITE`]; this pin is the two edges, which no corpus program can
/// carry because each one fails on purpose. A literal past `i128` is refused at
/// COMPILE time naming the value and the range (and the JS backend builds the
/// same program, which is what makes the refusal a backend limit rather than a
/// language one). An operation that leaves the range TRAPS at run time with the
/// same sentence, where the JS backend — arbitrary precision — simply answers
/// the bigger number; the pin reads both, so a native build that wrapped to a
/// negative would red.
#[test]
fn a_bigint_past_the_native_limit_is_refused_and_an_overflow_traps() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_bigint.vl"), BIGINT_LIMIT_PROBE)
        .expect("write the probe");
    let refused = vilan(&staged)
        .args([
            "build",
            "--backend",
            "rust",
            "--stdout",
            "native_probe_bigint.vl",
        ])
        .output()
        .expect("build the literal probe natively");
    assert!(!refused.status.success(), "the literal must be refused");
    let message = String::from_utf8_lossy(&refused.stderr);
    assert!(
        message.contains("is outside the native backend's range"),
        "the refusal names the rule: {message}"
    );
    assert!(
        message.contains("170141183460469231731687303715884105728"),
        "and the value it refused: {message}"
    );
    // The same program on the JS backend, where a `BigInt` really is arbitrary
    // precision — so this is a BACKEND limit and the message is honest.
    let javascript = vilan(&staged)
        .args(["run", "native_probe_bigint.vl"])
        .output()
        .expect("run the literal probe on the JS backend");
    assert!(javascript.status.success(), "the JS backend builds it");
    assert_eq!(
        String::from_utf8_lossy(&javascript.stdout),
        "170141183460469231731687303715884105728n\n"
    );

    std::fs::write(
        staged.join("native_probe_bigint_trap.vl"),
        BIGINT_TRAP_PROBE,
    )
    .expect("write the trap probe");
    let native = vilan(&staged)
        .args(["run", "--backend", "rust", "native_probe_bigint_trap.vl"])
        .output()
        .expect("run the trap probe natively");
    assert!(!native.status.success(), "the overflow ends the program");
    assert!(
        String::from_utf8_lossy(&native.stderr).contains("left the native backend's range"),
        "the trap names the rule: {}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        "170141183460469231731687303715884105727n\n",
        "the value INSIDE the range printed first, with node's `n`"
    );
    let javascript = vilan(&staged)
        .args(["run", "native_probe_bigint_trap.vl"])
        .output()
        .expect("run the trap probe on the JS backend");
    assert!(
        javascript.status.success(),
        "arbitrary precision does not trap"
    );
    assert_eq!(
        String::from_utf8_lossy(&javascript.stdout),
        "170141183460469231731687303715884105727n\n\
         170141183460469231731687303715884105728n\n"
    );
}

/// N120: a program whose arithmetic rustc can see overflowing at compile time
/// is refused by the native build — `deny(arithmetic_overflow)` — and the CLI
/// says so as the PROGRAM's overflow, naming the expression rustc underlined,
/// rather than accusing the backend. A conforming program does not overflow
/// (spec §7.2a; I5's ruling 2), and the JS backend runs on past one: the second
/// half of this pin is that very program printing on the JS backend.
///
/// Red before N120: the refusal ended in "`cargo build` refused the emitted
/// Rust. That is a BACKEND defect, not a defect in the vilan program".
#[test]
fn a_constant_overflow_is_reported_as_the_programs_not_the_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_overflow.vl"), OVERFLOW_PROBE)
        .expect("write the probe");
    let native = vilan(&staged)
        .args(["build", "--backend", "rust", "native_probe_overflow.vl"])
        .output()
        .expect("build the overflow probe natively");
    assert!(
        !native.status.success(),
        "rustc refuses the constant overflow"
    );
    let message = String::from_utf8_lossy(&native.stderr);
    assert!(
        message.contains("the PROGRAM overflows: rustc evaluated `((a_")
            // the emitted path is `src/main.rs` on unix and `src\main.rs` on windows
            && message.contains("+ (1i32)))` (the emitted Rust, src")
            && message.contains("main.rs:")
            && message.contains("attempt to compute `i32::MAX + 1_i32`, which would overflow")
            && message.contains("the JavaScript backend would have run on past it"),
        "the refusal names the program's overflow and the expression: {message}"
    );
    assert!(
        !message.contains("BACKEND defect"),
        "an overflow the program wrote is not a backend defect: {message}"
    );
    let javascript = vilan(&staged)
        .args(["run", "native_probe_overflow.vl"])
        .output()
        .expect("run the overflow probe on the JS backend");
    assert!(
        javascript.status.success(),
        "the JS backend runs on past it"
    );
    assert_eq!(String::from_utf8_lossy(&javascript.stdout), "2147483648\n");
}

const OVERFLOW_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun main() {\n",
    "\tlet a: i32 = 2147483647;\n",
    "\tprint(a + 1);\n",
    "}\n",
);

const BIGINT_LIMIT_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun main() {\n",
    "\tprint(170141183460469231731687303715884105728n);\n",
    "}\n",
);

const BIGINT_TRAP_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun main() {\n",
    "\tlet near = 170141183460469231731687303715884105727n;\n",
    "\tprint(near);\n",
    "\tprint(near + 1n);\n",
    "}\n",
);

/// **F18 slice 3's two seams**: node:crypto's SHA-1 behind `std::rpc_server`'s
/// `ws_accept_key`, and `std::time`'s host clock — the two host bindings, beside
/// F33's `Bytes`, that stood between `Server::builder()` and the native backend.
///
/// The accept key is RFC 6455 §1.3's own example (the client key
/// `dGhlIHNhbXBsZSBub25jZQ==` is answered `s3pPLMBiTxaQ9kYGzzhZRbK+xOo=`),
/// asserted verbatim as well as compared, so a native SHA-1 or base64 that
/// agreed with nothing would still red; the empty key is the second vector
/// because its digest is the one whose base64 ends in a single `=`. The clock
/// is compared only in what two processes a moment apart CAN agree on — it is
/// whole milliseconds since 1970 (the unit and the epoch, the two things a
/// wrong conversion gets wrong) and `now()` is not before it.
///
/// Red at the Order 40 seal (refused by name: `digest`, `now_millis`).
#[test]
fn the_websocket_accept_key_and_the_host_clock_agree_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_seams.vl"), SEAMS_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_seams.vl"),
        Verdict::Identical,
        "the accept key and the clock must print the same bytes on both backends"
    );
    let native = vilan(&staged)
        .args(["run", "--backend", "rust", "native_probe_seams.vl"])
        .output()
        .expect("run the native backend");
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\nKfh9QIsMVZcl6xEPYxPHzW8SZ8w=\ntrue true\ntrue\n",
        "RFC 6455's own accept key, and a clock in whole milliseconds since 1970"
    );
}

const SEAMS_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::rpc_server::ws_accept_key;\n",
    "import std::time::{ now, now_millis };\n",
    "\n",
    "fun main() {\n",
    "\tprint(ws_accept_key(\"dGhlIHNhbXBsZSBub25jZQ==\"));\n",
    "\tprint(ws_accept_key(\"\"));\n",
    "\tlet sampled = now_millis();\n",
    "\tprint(i\"{sampled > 1790000000000.0} {sampled == sampled.floor()}\");\n",
    "\tlet later = now();\n",
    "\tprint(i\"{later.millis >= sampled.as_i53()}\");\n",
    "}\n",
);

/// **F40 (RULED (a))**: `std::crypto`'s OS randomness, SHA-384/512, HMAC and
/// PBKDF2 natively, through the separate `vilan-rt-crypto` crate — and
/// node:crypto's `pbkdf2Sync` bound by the PROGRAM, with the `Buffer` it answers
/// declared as an `external struct` of the program's own naming and read back
/// through `toString(encoding)`, which is how kolt's `store.vl` hashes a
/// password.
///
/// Every deterministic line is compared byte for byte against node AND held
/// verbatim (each value is node's own answer), so a digest, an HMAC or a PBKDF2
/// that is wrong in any bit reds here; the random lines print only what two
/// processes can agree on — the lengths, that two draws differ, the UUID's
/// version nibble. The last line is `Shared::identity`'s stamp, which counts
/// from 1 in first-ask order on both backends (the native `i64` address it had
/// been did not even compile against std's `i32` declaration, which is the wall
/// kolt's server met past its host gaps).
///
/// And the reach is RECORDED: this program's manifest names `vilan-rt-crypto`,
/// and a program that reaches no crypto names neither optional crate.
///
/// Red before F40: refused by name (`random_bytes`, `sha384`, `sha512`,
/// `hmac_sha512`, `pbkdf2_sha512`, `pbkdf2_sync`, `to_string_encoded`).
#[test]
fn the_crypto_surface_answers_nodes_bytes_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_crypto.vl"), CRYPTO_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_crypto.vl"),
        Verdict::Identical,
        "the crypto surface must print the same bytes on both backends"
    );
    let native = vilan(&staged)
        .args(["run", "--backend", "rust", "native_probe_crypto.vl"])
        .output()
        .expect("run the native backend");
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        concat!(
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f\n",
            "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7\n",
            "164b7a7bfcf819e2e395fbe73b56e0a387bd64222e831fd610270cd7ea2505549758bf75c05a994a6d034f65f8f0e6fdcaeab1a34d4a6b4b636e070a38bce737\n",
            "e1d9c16aa681708a45f5c7c4e215ceb66e011a2e9f0040713f18aefdb866d53cf76cab2868a39b9f7840edce4fef5a82be67335c77a6068e04112754f27ccf4e\n",
            "9c549ce63c45f8df93229c0fac3d6457dc31b241409e21ef1b4e45c97c11001333ddb86821b04cb42fdfa9e3cb9996f4cee97ff6a7e62be799a5b23a83fc5a7f\n",
            "6mwBTcctb4zNHtkqzh1B8NjeiVc=\n",
            "rk0Mla9rRtMtCt_5KPBt0CowP47zwlHf1uLYWpVHTEM\n",
            "32 64 false\n",
            "36 4\n",
            "1 2 1\n",
        ),
        "node's own answers, and what two processes can agree on about randomness"
    );
    let manifest_of = |program: &str| {
        std::fs::read_to_string(
            staged
                .join("dist")
                .join("native")
                .join(program)
                .join("Cargo.toml"),
        )
        .expect("read the generated manifest")
    };
    let manifest = manifest_of("native_probe_crypto");
    assert!(
        manifest.contains("vilan-rt-crypto") && !manifest.contains("vilan-rt-sqlite"),
        "a program reaching `std::crypto`'s randomness names the crypto crate, and only it:\n\
         {manifest}"
    );
    let plain = vilan(&staged)
        .args(["build", "--backend", "rust", "bool.vl"])
        .output()
        .expect("build a program that reaches no optional crate");
    assert!(plain.status.success());
    let manifest = manifest_of("bool");
    assert!(
        !manifest.contains("vilan-rt-crypto") && !manifest.contains("vilan-rt-sqlite"),
        "a program that reaches neither names neither:\n{manifest}"
    );
}

const CRYPTO_PROBE: &str = concat!(
    "import std::bytes::encode_utf8;\n",
    "import std::crypto::{ hmac_sha512, pbkdf2_sha512, random_bytes, random_uuid, sha384, sha512 };\n",
    "import std::io::print;\n",
    "import std::shared::Shared;\n",
    "\n",
    "external struct HashBuffer;\n",
    "\n",
    "impl HashBuffer {\n",
    "\t[extern(method, \"toString\")]\n",
    "\texternal fun to_string_encoded(self, encoding: str): str;\n",
    "}\n",
    "\n",
    "[extern(\"node:crypto\", \"pbkdf2Sync\")]\n",
    "external fun pbkdf2_sync(password: str, salt: str, iterations: i32, key_length: i32, digest: str): HashBuffer;\n",
    "\n",
    "async fun main() {\n",
    "\tprint(sha512(encode_utf8(\"abc\")).to_hex());\n",
    "\tprint(sha384(encode_utf8(\"abc\")).to_hex());\n",
    "\tprint(hmac_sha512(encode_utf8(\"Jefe\"), encode_utf8(\"what do ya want for nothing?\")).to_hex());\n",
    "\tprint(pbkdf2_sha512(encode_utf8(\"password\"), encode_utf8(\"salt\"), 2, 512).to_hex());\n",
    "\tlet derived = pbkdf2_sync(\"lovelace1\", \"0123456789abcdef\", 100000, 64, \"sha512\");\n",
    "\tprint(derived.to_string_encoded(\"hex\"));\n",
    "\tprint(pbkdf2_sync(\"password\", \"salt\", 2, 20, \"sha1\").to_string_encoded(\"base64\"));\n",
    "\tprint(pbkdf2_sync(\"password\", \"salt\", 2, 32, \"SHA256\").to_string_encoded(\"base64url\"));\n",
    "\tlet first = random_bytes(32);\n",
    "\tlet second = random_bytes(32);\n",
    "\tprint(i\"{first.len()} {first.to_hex().len()} {first.to_hex() == second.to_hex()}\");\n",
    "\tlet uuid = random_uuid();\n",
    "\tprint(i\"{uuid.len()} {uuid.substring(14, 15)}\");\n",
    "\tlet cell = Shared::new(1);\n",
    "\tlet other = Shared::new(2);\n",
    "\tlet same = cell;\n",
    "\tprint(i\"{cell.identity()} {other.identity()} {same.identity()}\");\n",
    "}\n",
);

/// **F35's find — a native MISCOMPILE**: an assignment evaluates its place's
/// SUBSCRIPTS before its value, in source order, as JavaScript does.
///
/// Rust evaluates an assignment's right-hand side BEFORE its place, so
/// `list[next()] = next() * 10` gave the first draw to the value and the second
/// to the index, and wrote `10` into slot 2 where node writes `20` into slot 1 —
/// no error anywhere, a different answer. `list-cell.vl`'s random walk met it
/// (`edited[next_random(size)] = next_random(100)` edited a different element,
/// and `map_each` then ran `g` 1,105 times against node's 1,081). One line per
/// place shape: a subscript, two nested subscripts, a subscript under a field
/// write, and the compound form (B105's hoist, unchanged, as the control).
#[test]
fn an_assignment_evaluates_its_subscripts_before_its_value_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_order.vl"), ORDER_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_order.vl"),
        Verdict::Identical,
        "an assignment's order of evaluation must be the same on both backends"
    );
    let native = vilan(&staged)
        .args(["run", "--backend", "rust", "native_probe_order.vl"])
        .output()
        .expect("run the native backend");
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        "0 20 0 0\n0 5 0 0 0\n7 0\n9 20\n",
        "node's answers: every subscript drawn before the value"
    );
}

const ORDER_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::shared::Shared;\n",
    "\n",
    "let counter: Shared<i32> = Shared::new(0);\n",
    "\n",
    "fun next(): i32 {\n",
    "\tcounter.write() = counter.read() + 1;\n",
    "\tcounter.read()\n",
    "}\n",
    "\n",
    "struct Point {\n",
    "\tx: i32,\n",
    "\ty: i32,\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tmut list = [0, 0, 0, 0];\n",
    "\tlist[next()] = next() * 10;\n",
    "\tprint(i\"{list[0]} {list[1]} {list[2]} {list[3]}\");\n",
    "\tmut grid = [[0, 0, 0], [0, 0, 0], [0, 0, 0]];\n",
    "\tgrid[next() - 3][next() - 3] = next();\n",
    "\tprint(i\"{grid[0][0]} {grid[0][1]} {grid[1][0]} {grid[1][2]} {grid[2][1]}\");\n",
    "\tmut points = [Point { x = 0, y = 0 }, Point { x = 0, y = 0 }];\n",
    "\tpoints[next() - 6].x = next();\n",
    "\tprint(i\"{points[0].x} {points[1].x}\");\n",
    "\tlist[next() - 8] += next();\n",
    "\tprint(i\"{list[0]} {list[1]}\");\n",
    "}\n",
);

/// **F35**: the three lowering classes `ListCell` and `map_each` met past the
/// `any` wall the item was filed on (which an earlier order had already
/// lifted — the premise, re-measured, had moved):
///
/// 1. a NAMED FUNCTION in a value position (`apply(values, twice)`, `let named =
///    twice`) — the emitted instance behind the same counted handle a closure
///    literal is;
/// 2. a `&mut self` method whose later argument READS the receiver
///    (`self.place(self.size(), value)`, a trait default's `push` over
///    `splice`) — the value is evaluated ahead of the `&mut` borrow, since
///    Rust's two-phase borrow covers only an autoref method receiver (E0502);
/// 3. a `&mut` the source WROTE over a binding that is already a `&mut` loan
///    (`fill(&mut target)` with `target: &mut Stack`) — a reborrow, not a
///    `&mut &mut` (E0596).
///
/// Each line is node's answer; each class was a refusal or a rustc error
/// before F35.
#[test]
fn a_named_function_value_and_a_forwarded_loan_build_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_loans.vl"), LOANS_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_loans.vl"),
        Verdict::Identical,
        "the three classes must print the same bytes on both backends"
    );
    let native = vilan(&staged)
        .args(["run", "--backend", "rust", "native_probe_loans.vl"])
        .output()
        .expect("run the native backend");
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        "1 102 203 304\n2 4 6 42\n"
    );
}

const LOANS_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "struct Stack {\n",
    "\titems: List<i32>,\n",
    "}\n",
    "\n",
    "impl Stack {\n",
    "\tfun size(self): i32 {\n",
    "\t\tself.items.len().as_i32()\n",
    "\t}\n",
    "\n",
    "\tfun place(&mut self, at: i32, value: i32) {\n",
    "\t\tself.items.push(at * 100 + value);\n",
    "\t}\n",
    "\n",
    "\tfun push(&mut self, value: i32) {\n",
    "\t\tself.place(self.size(), value);\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun fill(target: &mut Stack) {\n",
    "\ttarget.push(1);\n",
    "\ttarget.push(2);\n",
    "}\n",
    "\n",
    "fun refill(target: &mut Stack) {\n",
    "\tfill(&mut target);\n",
    "\ttarget.push(3);\n",
    "}\n",
    "\n",
    "fun twice(value: i32): i32 {\n",
    "\tvalue * 2\n",
    "}\n",
    "\n",
    "fun apply(values: List<i32>, f: |i32| i32): List<i32> {\n",
    "\tvalues.map(f)\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tmut stack = Stack { items = [] };\n",
    "\trefill(&mut stack);\n",
    "\tstack.push(4);\n",
    "\tprint(i\"{stack.items[0]} {stack.items[1]} {stack.items[2]} {stack.items[3]}\");\n",
    "\tlet doubled = apply([1, 2, 3], twice);\n",
    "\tlet named = twice;\n",
    "\tprint(i\"{doubled[0]} {doubled[1]} {doubled[2]} {named(21)}\");\n",
    "}\n",
);

/// **F36 (a)**: interpolating a GENERIC call's result. The call's recorded type
/// is the callee's own parameter (`Source<T>::get`'s `T`), which is neither a
/// scalar nor a `str` until the call's substitution binds it — so every
/// `i"{source.get()}"` was refused as "a value with its own `render`". The
/// operand is now judged at the type THIS call binds (`i32`, `str`, `bool`),
/// through the substitution the call itself is emitted under.
///
/// Written over `Source` BOUNDS, not `SignalCell` return types, so reactive-42's
/// combinator flip (`map` answering a node) keeps it green: `show` and `shout`
/// take any `S: Source<..>`, and the derived value is only ever handed to one.
#[test]
fn a_generic_calls_result_interpolates_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_interpolate.vl"),
        INTERPOLATE_PROBE,
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_interpolate.vl"),
        Verdict::Identical,
        "an interpolated generic call must print the same bytes on both backends"
    );
    let native = vilan(&staged)
        .args(["run", "--backend", "rust", "native_probe_interpolate.vl"])
        .output()
        .expect("run the native backend");
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        "count 1\ndoubled 10\nname ada <ada> true 6\n"
    );
}

const INTERPOLATE_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::reactive::{ SignalCell, Source };\n",
    "\n",
    "fun show<S: Source<i32>>(label: str, source: S) {\n",
    "\tprint(i\"{label} {source.get()}\");\n",
    "}\n",
    "\n",
    "fun shout<S: Source<str>>(source: S): str {\n",
    "\ti\"<{source.get()}>\"\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet count = SignalCell::new(1);\n",
    "\tshow(\"count\", count);\n",
    "\tlet doubled = count.derive(|n| n * 2).memo();\n",
    "\tcount.set(5);\n",
    "\tshow(\"doubled\", doubled);\n",
    "\tlet name = SignalCell::new(\"ada\");\n",
    "\tlet flag = SignalCell::new(true);\n",
    "\tprint(i\"name {name.get()} {shout(name)} {flag.get()} {count.get() + 1}\");\n",
    "}\n",
);

/// **F36 (b), premise CORRECTED**: `encode_json` over a `List<i32>` — the item
/// recorded rustc's E0596 (a `&mut` through an immutable binding), and on this
/// base every shape re-measured builds and prints node's bytes: a local, a
/// `mut` local, a parameter, a field through `self`, a call's result, a
/// closure's parameter, a literal, a `List<usize>`, a cell's value, a nested
/// list, and a `[derive(Wire)]` struct holding one. The lowering that closed it
/// is the loan REBORROW native-41 landed for `[derive(Wire)]`'s `describe`
/// handing its serializer on; this is its regression pin, green from the
/// first run — there was nothing left to turn red.
#[test]
fn encode_json_over_a_list_prints_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_encode.vl"), ENCODE_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_encode.vl"),
        Verdict::Identical,
        "`encode_json` over a list must print the same bytes on both backends"
    );
}

const ENCODE_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::json::encode_json;\n",
    "import std::reactive::{ SignalCell, Source };\n",
    "import std::wire::Wire;\n",
    "\n",
    "[derive(Wire)]\n",
    "struct Bag {\n",
    "\tvalues: List<i32>,\n",
    "\tname: str,\n",
    "}\n",
    "\n",
    "impl Bag {\n",
    "\tfun text(self): str {\n",
    "\t\tencode_json(self.values)\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun make(): List<i32> {\n",
    "\t[4, 5]\n",
    "}\n",
    "\n",
    "fun show(values: List<i32>): str {\n",
    "\tencode_json(values)\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet values: List<i32> = [1, 2, 3];\n",
    "\tprint(encode_json(values));\n",
    "\tmut grown: List<i32> = [1];\n",
    "\tgrown.push(2);\n",
    "\tprint(encode_json(grown));\n",
    "\tprint(show(values));\n",
    "\tlet bag = Bag { values, name = \"h\" };\n",
    "\tprint(bag.text());\n",
    "\tprint(encode_json(bag));\n",
    "\tprint(encode_json(make()));\n",
    "\tlet render = |list: List<i32>| encode_json(list);\n",
    "\tprint(render([9]));\n",
    "\tprint(encode_json([1, 2, 3]));\n",
    "\tlet sizes: List<usize> = [1, 2];\n",
    "\tprint(encode_json(sizes));\n",
    "\tlet cell = SignalCell::new([6, 7]);\n",
    "\tprint(encode_json(cell.get()));\n",
    "\tlet nested: List<List<i32>> = [[1], [2, 3]];\n",
    "\tprint(encode_json(nested));\n",
    "}\n",
);

/// F36's boundary, CLOSED by B425: `blanket-impl.vl` met a call that threaded
/// FEWER context arguments than its callee's instance declared
/// (`badge("static")` from `main`). The instance declared the ambient `Owner`
/// because the context pass read `label.bind(..)` — a dispatch through `V:
/// MaybeSignal<str>`, the program's OWN trait — as reaching every member
/// NAMED `bind`, std's `MaybeSignal::bind` (whose reactive impl registers an
/// effect) among them, while coverage, which narrows, found no need. A
/// generic-member site's candidates are now the bound's traits' members only,
/// so nothing is declared that no caller supplies: the program is identical
/// on both backends (it was refused by name here, and before that rustc's
/// E0061).
#[test]
fn a_call_missing_a_context_argument_is_refused_by_name() {
    let staged = stage();
    assert_eq!(
        compare(&staged, "blanket-impl.vl"),
        Verdict::Identical,
        "`blanket-impl.vl` must mean the same thing on both backends"
    );
}

/// **F38**: `and_then<U>`'s `U` — a callee's OWN generic parameter that no
/// written argument names and the analyzer records nothing for — is closed
/// from the closure the call is handed: the declared `|T| Result<U, E>`
/// against the literal's parameters and its body's tail. A tail the analyzer
/// typed nowhere (`Ok(n * 2)`) is read through the constructor's arguments,
/// an arithmetic operand through its left side, a concatenation as `str`.
/// `Result` and `Option` both, chained, on the `Err` path, and at a `U` that is
/// not the receiver's `T` (`str`, `bool`).
///
/// Red before F38: refused by name, "a value of an unbound generic type
/// parameter (parameter 1 of `and_then`)".
#[test]
fn and_thens_own_parameter_is_closed_from_its_closure_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_and_then.vl"), AND_THEN_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_and_then.vl"),
        Verdict::Identical,
        "`and_then` must print the same bytes on both backends"
    );
    let native = vilan(&staged)
        .args(["run", "--backend", "rust", "native_probe_and_then.vl"])
        .output()
        .expect("run the native backend");
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        "20\n-1\n-2\ntext 10\n4\ntrue\n"
    );
}

const AND_THEN_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun half(n: i32): Result<i32, str> {\n",
    "\tif n % 2 == 0 {\n",
    "\t\tOk(n / 2)\n",
    "\t} else {\n",
    "\t\tErr(i\"odd {n}\")\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet ok: Result<i32, str> = Ok(10);\n",
    "\tlet err: Result<i32, str> = Err(\"boom\");\n",
    "\tprint(ok.and_then(|n| Ok(n * 2)).unwrap_or(0));\n",
    "\tprint(err.and_then(|n| Ok(n * 2)).unwrap_or(-1));\n",
    "\tprint(ok.and_then(|n| half(n)).and_then(|n| half(n)).unwrap_or(-2));\n",
    "\tprint(ok.and_then(|n| Ok(i\"text {n}\")).unwrap_or(\"none\"));\n",
    "\tlet some: Option<i32> = Some(3);\n",
    "\tprint(some.and_then(|n| Some(n + 1)).unwrap_or(0));\n",
    "\tprint(some.and_then(|n| Some(n > 2)).unwrap_or(false));\n",
    "}\n",
);

/// F38's boundary, moved by B424 and closed by B454: `result-combinators.vl`
/// stopped at `or_else<F>` over an `Ok`-only closure (`err.or_else(|e|
/// Ok(7))`), whose `F` nothing in the program constrained — the ruling (R-h,
/// door (b)) gives that `F` the input's error type, and `err.or(Ok(3))`'s
/// likewise — and then at `ok.and(Ok(5))`, whose result lost the receiver's
/// `E`: the argument, typed in `Result`'s own terms, reconciled `E` back to
/// itself over the receiver's `str` (a self-binding is no evidence now). The
/// whole program builds natively and prints node's bytes.
#[test]
fn result_combinators_is_identical_on_both_backends() {
    let staged = stage();
    assert_eq!(
        compare(&staged, "result-combinators.vl"),
        Verdict::Identical,
        "every combinator of `result-combinators.vl` must build natively and agree"
    );
}

/// **F37**: a PARTIAL move where the source's order asks for one, and the
/// conservative copies the last-use rule used to take at a field or on an
/// exclusive branch.
///
/// The item's own repro (`let h = r.live; push(copy_of(r))`) already built on
/// this base — native-41's consumed-FIELD copy covered it, conservatively.
/// What F37 builds is the PRECISION: the liveness pass reads a field by its
/// PATH (a field read is its field's last use when every later read of the
/// binding touches a disjoint field, so `Rec { live = r.live, name = r.name,
/// n = r.n }` moves all three), and it forks at an `if`/`else` chain and an
/// unguarded `match` (a read that is last on its own exclusive path moves).
/// `keep` is the attach order reactive-41 had to write around — the handle
/// built first from two fields, the record handed on whole as its last use —
/// and it builds and answers node's bytes. The overlap cases keep their copies:
/// a field read twice, and a nested path followed by its parent.
///
/// The copy count is held too: 5 copied / 5 moved. With the path and the
/// branch halves planted out ("a field never moves, and branches are walked
/// in sequence") the same program takes 7 copies and moves 1. (It read 5 / 3
/// until native-45 made a `match` leg's body and a closure's body consuming
/// positions (F63, F64): two reads that were already moves — a leg handing
/// back a binding at its last use — are counted as elided copies now. No copy
/// was added.)
#[test]
fn a_field_and_a_branch_read_move_at_their_own_last_use_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_moves.vl"), MOVES_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_moves.vl"),
        Verdict::Identical,
        "partial moves must print the same bytes on both backends"
    );
    let native = vilan(&staged)
        .args(["run", "--backend", "rust", "native_probe_moves.vl"])
        .output()
        .expect("run the native backend");
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        "a 1 false\nb 2\ncc\nxxy\nd!\ne\nf\n"
    );
    assert_eq!(
        copy_census_of(&staged, "native_probe_moves.vl"),
        (5, 5),
        "the copies the path- and branch-aware last use leaves"
    );
}

const MOVES_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::shared::Shared;\n",
    "\n",
    "struct Rec {\n",
    "\tlive: Shared<bool>,\n",
    "\tname: str,\n",
    "\tn: i32,\n",
    "}\n",
    "\n",
    "struct Handle {\n",
    "\tlive: Shared<bool>,\n",
    "\tname: str,\n",
    "}\n",
    "\n",
    "struct Inner {\n",
    "\tlabel: str,\n",
    "}\n",
    "\n",
    "struct Outer {\n",
    "\tinner: Inner,\n",
    "\ttag: str,\n",
    "}\n",
    "\n",
    "// The attach order reactive-41 had to avoid: the handle built FIRST from two\n",
    "// fields, then the record handed on whole as its last use.\n",
    "fun keep(record: Rec, into: Shared<List<Rec>>): Handle {\n",
    "\tlet made = Handle { live = record.live, name = record.name };\n",
    "\tinto.write().push(record);\n",
    "\tmade\n",
    "}\n",
    "\n",
    "// Four disjoint fields, each its own last use.\n",
    "fun rebuild(record: Rec): Rec {\n",
    "\tRec { live = record.live, name = record.name, n = record.n }\n",
    "}\n",
    "\n",
    "// An overlapping later read keeps the earlier copy: the name is read twice.\n",
    "fun twice(record: Rec): str {\n",
    "\tlet first = record.name;\n",
    "\tfirst + record.name\n",
    "}\n",
    "\n",
    "// A nested path, then its parent — the parent read overlaps.\n",
    "fun nested(outer: Outer): str {\n",
    "\tlet label = outer.inner.label;\n",
    "\tlet inner = outer.inner;\n",
    "\tlabel + inner.label + outer.tag\n",
    "}\n",
    "\n",
    "// Exclusive arms: each arm's read is its path's last.\n",
    "fun choose(record: Rec, flag: bool): str {\n",
    "\tif flag {\n",
    "\t\trecord.name\n",
    "\t} else if record.n > 1 {\n",
    "\t\trecord.name + \"!\"\n",
    "\t} else {\n",
    "\t\t\"none\"\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun pick(value: Option<Rec>, fallback: Rec): Rec {\n",
    "\tmatch value {\n",
    "\t\tSome(let found) => found,\n",
    "\t\tNone => fallback,\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet into: Shared<List<Rec>> = Shared::new([]);\n",
    "\tlet handle = keep(Rec { live = Shared::new(true), name = \"a\", n = 1 }, into);\n",
    "\thandle.live.write() = false;\n",
    "\tprint(i\"{handle.name} {into.read().len()} {into.read()[0].live.read()}\");\n",
    "\tlet again = rebuild(Rec { live = Shared::new(true), name = \"b\", n = 2 });\n",
    "\tprint(i\"{again.name} {again.n}\");\n",
    "\tprint(twice(Rec { live = Shared::new(true), name = \"c\", n = 3 }));\n",
    "\tprint(nested(Outer { inner = Inner { label = \"x\" }, tag = \"y\" }));\n",
    "\tprint(choose(Rec { live = Shared::new(true), name = \"d\", n = 2 }, false));\n",
    "\tprint(choose(Rec { live = Shared::new(true), name = \"e\", n = 2 }, true));\n",
    "\tprint(pick(None, Rec { live = Shared::new(true), name = \"f\", n = 0 }).name);\n",
    "}\n",
);

/// **F39 (decided: the compile-time check follows aliases)**: a reentrant read
/// through an ALIAS of the cell being `update`d is refused by name, as the
/// same-place read already was — `let t2 = todos;` copies the cell's HANDLES,
/// so `t2.get()` inside `todos.update(..)` is the same cell, and so is the
/// reverse. The control stays identical: a VALUE read out of the cell before
/// the update (`let before = todos.get();`) is a copy of the list, not an
/// alias of the cell, and a field alias of a DIFFERENT cell is not the cell.
#[test]
fn a_reentrant_read_through_an_alias_is_refused_by_name() {
    let staged = stage();
    for (name, body) in [
        (
            "native_probe_alias_forward.vl",
            "\tlet t2 = todos;\n\ttodos.update(|&mut list| {\n\t\tlist.push(2);\n\t\tprint(t2.get().len());\n\t});\n",
        ),
        (
            "native_probe_alias_reverse.vl",
            "\tlet t2 = todos;\n\tt2.update(|&mut list| {\n\t\tlist.push(2);\n\t\tprint(todos.get().len());\n\t});\n",
        ),
        (
            "native_probe_alias_chain.vl",
            "\tlet t2 = todos;\n\tlet t3 = t2;\n\tt3.update(|&mut list| {\n\t\tlist.push(2);\n\t\tprint(todos.get().len());\n\t});\n",
        ),
    ] {
        std::fs::write(
            staged.join(name),
            format!(
                "import std::io::print;\nimport std::reactive::{{ Signal, SignalCell }};\n\nfun main() {{\n\tlet todos: SignalCell<List<i32>> = Signal::new([1]);\n{body}}}\n"
            ),
        )
        .expect("write the probe");
        match compare(&staged, name) {
            Verdict::Refused(reason) => assert!(
                reason.contains("reads the same place again"),
                "{name}: refused, and for this reason: {reason}"
            ),
            other => panic!("{name}: a reentrant read through an alias must be refused: {other:?}"),
        }
    }
    std::fs::write(
        staged.join("native_probe_alias_value.vl"),
        concat!(
            "import std::io::print;\n",
            "import std::reactive::{ Signal, SignalCell };\n",
            "\n",
            "fun main() {\n",
            "\tlet todos: SignalCell<List<i32>> = Signal::new([1]);\n",
            "\tlet before = todos.get();\n",
            "\ttodos.update(|&mut list| {\n",
            "\t\tlist.push(before.len().as_i32() + 1);\n",
            "\t});\n",
            "\tprint(todos.get());\n",
            "}\n",
        ),
    )
    .expect("write the control");
    assert_eq!(
        compare(&staged, "native_probe_alias_value.vl"),
        Verdict::Identical,
        "a value read before the update is not an alias of the cell"
    );
}

/// **F39's runtime half**: an alias no static walk can see — the SAME cell
/// handed to two parameters — still stops natively (the JS backend answers
/// the in-progress value, which safe Rust has no second view of storage to
/// read), but with the runtime's own sentence and node's exit code, not
/// Rust's `already mutably borrowed`. Outside the differential by
/// construction: the two backends answer differently, and that is the claim.
#[test]
fn a_reentrant_read_the_compiler_cannot_see_stops_with_the_runtimes_sentence() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_alias_parameter.vl"),
        concat!(
            "import std::io::print;\n",
            "import std::reactive::{ Signal, SignalCell };\n",
            "\n",
            "fun touch(a: SignalCell<List<i32>>, b: SignalCell<List<i32>>) {\n",
            "\ta.update(|&mut list| {\n",
            "\t\tlist.push(2);\n",
            "\t\tprint(b.get().len());\n",
            "\t});\n",
            "}\n",
            "\n",
            "fun main() {\n",
            "\tlet todos: SignalCell<List<i32>> = Signal::new([1]);\n",
            "\ttouch(todos, todos);\n",
            "}\n",
        ),
    )
    .expect("write the probe");
    let native = vilan(&staged)
        .args([
            "run",
            "--backend",
            "rust",
            "native_probe_alias_parameter.vl",
        ])
        .output()
        .expect("run the native backend");
    assert_eq!(
        native.status.code(),
        Some(1),
        "node's exit code for a throw"
    );
    let stderr = String::from_utf8_lossy(&native.stderr);
    assert!(
        stderr.contains("a cell was read while it is being updated"),
        "the runtime names the shape:\n{stderr}"
    );
    assert!(
        !stderr.contains("already mutably borrowed"),
        "not Rust's own sentence:\n{stderr}"
    );
    let javascript = vilan(&staged)
        .args(["run", "native_probe_alias_parameter.vl"])
        .output()
        .expect("run the JS backend");
    assert_eq!(
        String::from_utf8_lossy(&javascript.stdout),
        "2\n",
        "the JS backend answers the in-progress value"
    );
}

/// **C14, re-scoped: the NATIVE LEAK GATE** — the counted-`Shared` census S4's
/// exit test became once F1 S1a made `Shared`/`Weak` a real `Rc`/`Weak`
/// natively (retain on clone, release on drop, by construction).
///
/// `VILAN_NATIVE_LEAK_CENSUS=1` runs the program on a thread of its own and,
/// after that thread has exited — `main`'s frame gone, the event loop drained,
/// every thread-local (the module-level bindings, the executor's queues)
/// destroyed — prints how many cells it minted and how many are still live.
/// What is live then is what NOTHING can release: a cycle of counted cells.
///
/// The table is [`DEFAULT_SUITE`] plus the board probe, as the copy census is.
/// Its claim is `live = 0` wherever the program's cells form no cycle; a
/// non-zero row is a leak, found, and named in the report that moved it —
/// not a number to regenerate past. Regenerate with
/// `VILAN_REGENERATE_NATIVE_LEAK_CENSUS=1` only after reading the difference.
///
/// The F18/F40 exit — kolt's server and its shape — is not a row of THIS
/// table, whose rows run to their own end: a server runs until it is told to
/// stop. Since F45 it can be told — SIGTERM stops it gracefully and the
/// census reads it at process end — and the kolt shape's exit line is held by
/// [`a_native_server_stops_on_sigterm_and_reaches_its_process_end`].
#[test]
fn the_native_leak_census_matches_its_table() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_board.vl"), BOARD_PROBE)
        .expect("write the board probe");
    let mut rows = Vec::new();
    for program in DEFAULT_SUITE
        .iter()
        .copied()
        .chain(std::iter::once("native_probe_board.vl"))
    {
        let (minted, live) = leak_census_of(&staged, program);
        rows.push(format!(
            "{}\t{minted}\t{live}",
            program.trim_end_matches(".vl")
        ));
    }
    let measured = format!(
        "{}{}\n",
        concat!(
            "# Counted cells each program MINTED, and the ones still LIVE after its\n",
            "# thread (and every thread-local) is gone: a live cell is a cycle (C14's\n",
            "# native leak gate). Regenerate with\n",
            "# VILAN_REGENERATE_NATIVE_LEAK_CENSUS=1 cargo test -p vilan-cli \
             --test native_differential\n",
        ),
        rows.join("\n")
    );
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(NATIVE_LEAK_CENSUS);
    if std::env::var_os("VILAN_REGENERATE_NATIVE_LEAK_CENSUS").is_some() {
        std::fs::write(&path, &measured).expect("write the census");
        return;
    }
    let committed = std::fs::read_to_string(&path).expect("read the committed census");
    assert_eq!(
        committed, measured,
        "the native leak census moved; read the difference, then regenerate with \
         VILAN_REGENERATE_NATIVE_LEAK_CENSUS=1"
    );
}

const NATIVE_LEAK_CENSUS: &str = "crates/vilan-cli/tests/native-leak-census.tsv";

/// Runs `program` natively under `VILAN_NATIVE_LEAK_CENSUS=1` and answers
/// `(minted, live)` from the line the runtime prints on stderr.
fn leak_census_of(staged: &Path, program: &str) -> (u64, u64) {
    let output = vilan(staged)
        .env("VILAN_NATIVE_LEAK_CENSUS", "1")
        .args(["run", "--backend", "rust", program])
        .output()
        .expect("run the program natively");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr
        .lines()
        .find(|line| line.starts_with("vilan-native: cells minted="))
        .unwrap_or_else(|| panic!("{program} reported no leak census:\n{stderr}"));
    let mut numbers = line
        .split(|character: char| !character.is_ascii_digit())
        .filter(|piece| !piece.is_empty())
        .map(|piece| piece.parse::<u64>().expect("a count"));
    let minted = numbers.next().expect("the minted count");
    let live = numbers.next().expect("the live count");
    (minted, live)
}

/// The leak gate's instrument, held both ways: a program whose cells form no
/// cycle — module-level cells included, which are released with their
/// thread-locals — ends with none live, and a program that closes ONE cycle (a
/// `Link` whose `next` cell is written to hold the link itself) ends with
/// exactly that cell live. A census that could not see a cycle would pass the first and
/// fail the second.
#[test]
fn the_leak_census_sees_a_cycle_and_nothing_else() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_no_cycle.vl"),
        concat!(
            "import std::io::print;\n",
            "import std::shared::Shared;\n",
            "\n",
            "let counter: Shared<i32> = Shared::new(0);\n",
            "\n",
            "fun main() {\n",
            "\tlet cell: Shared<List<|| i32>> = Shared::new([]);\n",
            "\tlet other: Shared<i32> = Shared::new(3);\n",
            "\tcell.write().push(|| other.read());\n",
            "\tcounter.write() = counter.read() + 1;\n",
            "\tprint(cell.read().len());\n",
            "}\n",
        ),
    )
    .expect("write the acyclic probe");
    std::fs::write(
        staged.join("native_probe_cycle.vl"),
        concat!(
            "import std::io::print;\n",
            "import std::option::Option::{ self, None, Some };\n",
            "import std::shared::Shared;\n",
            "\n",
            "struct Link {\n",
            "\tname: str,\n",
            "\tnext: Shared<Option<Link>>,\n",
            "}\n",
            "\n",
            "fun main() {\n",
            "\tlet link = Link { name = \"loop\", next = Shared::new(None) };\n",
            "\tlink.next.write() = Some(link);\n",
            "\tprint(link.name);\n",
            "}\n",
        ),
    )
    .expect("write the cyclic probe");
    let (minted, live) = leak_census_of(&staged, "native_probe_no_cycle.vl");
    assert_eq!(live, 0, "no cycle, nothing live ({minted} minted)");
    let (_, live) = leak_census_of(&staged, "native_probe_cycle.vl");
    assert_eq!(live, 1, "the one cell the closure closes a cycle through");
}

/// **F18 slice 2**: a closure declared SYNCHRONOUS, answering nothing, whose
/// body awaits.
///
/// node drops the promise such a callback returns — the call site does not
/// wait, and the body finishes later — so the native backend SPAWNS the body
/// and the closure answers `()`. `std::http`'s `upgrade_handler` is the shape
/// this was built for (`|NodeRequest, NodeSocket, Bytes| void`, with A40's
/// `authorize` hook awaiting inside it).
///
/// The pin is the ORDER, which is the whole claim: `before`, `after`, then the
/// handler's line, because the call returns before the awaited body resumes.
/// A backend that simply ran the body to completion at the call would print
/// them in a different order and still "work".
///
/// Non-vacuous by its neighbour: `adapt.vl`, whose closures answer a VALUE,
/// stays refused by name in [`every_async_corpus_program_is_identical_or_named`]
/// — and the first spelling of the void test floated those three too and was
/// caught there by `expected i32, found ()`.
#[test]
fn a_void_closure_whose_body_awaits_floats_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_float.vl"), FLOAT_PROBE).expect("write the probe");
    assert_eq!(
        compare(&staged, "native_probe_float.vl"),
        Verdict::Identical
    );
    let javascript = vilan(&staged)
        .args(["run", "native_probe_float.vl"])
        .output()
        .expect("run the JS backend");
    assert_eq!(
        String::from_utf8_lossy(&javascript.stdout),
        "before\nafter\nhandled 7\n",
        "the call returns BEFORE the awaited body resumes"
    );
}

const FLOAT_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::time::sleep;\n",
    "\n",
    "struct Sink {\n",
    "\ton_event: |i32| void,\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet sink = Sink {\n",
    "\t\ton_event = |value| {\n",
    "\t\t\tsleep(1);\n",
    "\t\t\tprint(i\"handled {value}\");\n",
    "\t\t},\n",
    "\t};\n",
    "\tprint(\"before\");\n",
    "\t(sink.on_event)(7);\n",
    "\tprint(\"after\");\n",
    "}\n",
);

#[test]
fn a_failing_program_exits_one_on_both_backends_without_rusts_banner() {
    let staged = stage();
    for (file, source, expected_stdout) in [
        ("native_probe_panic.vl", PANIC_PROBE, "before the panic\n"),
        (
            "native_probe_panic_async.vl",
            ASYNC_PANIC_PROBE,
            "before the async panic\n",
        ),
    ] {
        std::fs::write(staged.join(file), source).expect("write the probe program");
        let native = vilan(&staged)
            .args(["run", "--backend", "rust", file])
            .output()
            .expect("run the failing probe natively");
        let javascript = vilan(&staged)
            .args(["run", file])
            .output()
            .expect("run the failing probe on the JS backend");
        assert_eq!(
            native.status.code(),
            Some(1),
            "{file}: the native binary must exit 1, not Rust's 101:\n{}",
            String::from_utf8_lossy(&native.stderr)
        );
        assert_eq!(
            javascript.status.code(),
            Some(1),
            "{file}: the JS leg is the oracle and it exits 1"
        );
        assert_eq!(
            String::from_utf8_lossy(&native.stdout),
            String::from_utf8_lossy(&javascript.stdout),
            "{file}: stdout up to the failure must still be identical"
        );
        assert_eq!(
            String::from_utf8_lossy(&native.stdout),
            expected_stdout,
            "{file}: the probe must get as far as its own output, or this pin is \
             asserting nothing about the failure"
        );
        let stderr = String::from_utf8_lossy(&native.stderr);
        assert!(
            !stderr.contains("panicked at"),
            "{file}: Rust's panic banner must not reach stderr: {stderr:?}"
        );
        assert!(
            !stderr.contains("RUST_BACKTRACE"),
            "{file}: Rust's backtrace note must not reach stderr: {stderr:?}"
        );
        // ONE line about the failure, and it is the program's own message —
        // node prints one too. Two was the shape before the executor stopped
        // reporting the root task as an unobserved failure.
        let said: Vec<&str> = stderr.lines().filter(|line| !line.is_empty()).collect();
        assert_eq!(
            said.len(),
            1,
            "{file}: one line about one failure: {said:?}"
        );
        assert!(
            said[0].contains("boom"),
            "{file}: and it is the program's own message: {said:?}"
        );
    }
}

const PANIC_PROBE: &str = concat!(
    "import std::io::{ print, panic };\n",
    "\n",
    "fun main() {\n",
    "\tprint(\"before the panic\");\n",
    "\tpanic(\"boom\");\n",
    "}\n",
);

const ASYNC_PANIC_PROBE: &str = concat!(
    "import std::io::{ print, panic };\n",
    "import std::time::sleep;\n",
    "\n",
    "async fun main() {\n",
    "\tprint(\"before the async panic\");\n",
    "\tsleep(1);\n",
    "\tpanic(\"boom\");\n",
    "}\n",
);

/// F25: printing a host handle or a value holding a function is refused where
/// it is WRITTEN.
///
/// Order 38 answered both with a runtime panic carrying the reason, which is
/// honest and one release too late — what node prints there is its own object
/// inspection (`Promise { <pending> }`, `[Function (anonymous)]`), so the
/// program cannot work and nothing is gained by letting it build.
#[test]
fn printing_a_host_handle_or_a_function_is_refused_at_compile_time() {
    let staged = stage();
    for (file, source, needle) in [
        (
            "native_probe_print_task.vl",
            PRINT_HANDLE_PROBE,
            "`print` of the host handle `Task`",
        ),
        (
            "native_probe_print_fn.vl",
            PRINT_FUNCTION_PROBE,
            "`print` of a value holding a function",
        ),
    ] {
        std::fs::write(staged.join(file), source).expect("write the probe program");
        let output = vilan(&staged)
            .args(["build", "--backend", "rust", "--stdout", file])
            .output()
            .expect("build the probe");
        let message = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "{file} must be refused rather than built"
        );
        assert!(
            message.contains(needle),
            "{file} must be refused by name (`{needle}`); it said:\n{message}"
        );
        // Non-vacuous: the JS backend BUILDS the same program, so the refusal
        // is the native backend's answer and not a defect in the probe.
        let javascript = vilan(&staged)
            .args(["build", file])
            .output()
            .expect("build the probe on the JS backend");
        assert!(
            javascript.status.success(),
            "{file} must be a program the JS backend accepts:\n{}",
            String::from_utf8_lossy(&javascript.stderr)
        );
    }
}

const PRINT_HANDLE_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::time::sleep;\n",
    "\n",
    "async fun work(): i32 {\n",
    "\tsleep(1);\n",
    "\t7\n",
    "}\n",
    "\n",
    "async fun main() {\n",
    "\tlet task = async work();\n",
    "\tprint(task);\n",
    "}\n",
);

const PRINT_FUNCTION_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun main() {\n",
    "\tlet f = |x: i32| x + 1;\n",
    "\tprint(f);\n",
    "}\n",
);

/// A124 R3: the two object shapes the native backend does not lower are
/// refused BY NAME, and the JS backend builds both — so each refusal is the
/// native answer, not a broken probe. A `&mut self` slot would write through a
/// counted pointer every copy of the object shares (the JS backend copies the
/// pair instead); an async member's slot would answer a future no object slot
/// is built to carry.
#[test]
fn an_object_the_native_backend_cannot_lower_is_refused_by_name() {
    let staged = stage();
    for (file, source, needle) in [
        (
            "native_probe_dyn_mut.vl",
            DYN_MUT_SELF_PROBE,
            "`Counter::bump` through a `dyn` object (a `&mut self` slot",
        ),
        (
            "native_probe_dyn_async.vl",
            DYN_ASYNC_PROBE,
            "the async member `Fetch::get` through a `dyn` object",
        ),
    ] {
        std::fs::write(staged.join(file), source).expect("write the probe program");
        let output = vilan(&staged)
            .args(["build", "--backend", "rust", "--stdout", file])
            .output()
            .expect("build the probe");
        let message = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "{file} must be refused rather than built"
        );
        assert!(
            message.contains(needle),
            "{file} must be refused by name (`{needle}`); it said:\n{message}"
        );
        let javascript = vilan(&staged)
            .args(["build", file])
            .output()
            .expect("build the probe on the JS backend");
        assert!(
            javascript.status.success(),
            "{file} must be a program the JS backend accepts:\n{}",
            String::from_utf8_lossy(&javascript.stderr)
        );
    }
}

const DYN_MUT_SELF_PROBE: &str = concat!(
    "trait Counter { fun get(self): i32; fun bump(&mut self): void; }\n",
    "struct C { n: i32 }\n",
    "impl C with Counter {\n",
    "\tfun get(self): i32 { self.n }\n",
    "\tfun bump(&mut self): void { self.n = self.n + 1; }\n",
    "}\n",
    "fun main() {\n",
    "\tmut a: dyn Counter = C { n = 1 };\n",
    "\ta.bump();\n",
    "\tprint(a.get());\n",
    "}\n",
);

const DYN_ASYNC_PROBE: &str = concat!(
    "trait Fetch { async fun get(self): str; }\n",
    "struct Local { u: str }\n",
    "impl Local with Fetch { fun get(self): str { \"local\" } }\n",
    "fun show(f: dyn Fetch) { print(f.get()); }\n",
    "fun main() { show(Local { u = \"b\" }); }\n",
);

/// S1b's monomorphisation, held to the shape rather than to one program: a
/// generic function, a generic struct and a generic enum each emit ONE Rust
/// item per instantiation, and two instantiations of one declaration are two
/// distinct items.
///
/// Written as an inline probe rather than over the corpus because the corpus
/// has no program that instantiates one declaration at two types AND prints
/// both — which is precisely the case a single-instance emitter would pass.
#[test]
fn one_declaration_at_two_types_emits_two_rust_items() {
    let staged = stage();
    let probe = staged.join("native_probe_mono.vl");
    std::fs::write(&probe, MONO_PROBE).expect("write the probe program");
    let output = vilan(&staged)
        .args([
            "build",
            "--backend",
            "rust",
            "--stdout",
            "native_probe_mono.vl",
        ])
        .output()
        .expect("build the probe");
    let source = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "the monomorphisation probe was refused:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Two instances of the function, two of the struct, two of the enum. The
    // names carry the declaration's id and a per-declaration sequence number,
    // so counting the DECLARED items is counting the instances.
    let count = |needle: &str| source.matches(needle).count();
    assert_eq!(
        count("fn identity_"),
        2,
        "two instances of `identity`:\n{source}"
    );
    assert_eq!(
        count("struct Pair_"),
        2,
        "two instances of `Pair`:\n{source}"
    );
    assert_eq!(count("enum Tree_"), 2, "two instances of `Tree`:\n{source}");
    // And the instantiations are really distinct: one `Pair` holds an i32 left,
    // the other a string one.
    assert!(source.contains("left: i32"), "{source}");
    assert!(source.contains("left: vilan_rt::Str"), "{source}");
    // A NON-generic declaration keeps S1a's plain `{name}_{id}` — the property
    // that leaves every program the previous slice emitted byte-identical.
    assert!(
        source.contains("fn plain_"),
        "a non-generic function keeps its unsuffixed name:\n{source}"
    );

    let run = vilan(&staged)
        .args(["run", "--backend", "rust", "native_probe_mono.vl"])
        .output()
        .expect("run the probe natively");
    let javascript = vilan(&staged)
        .args(["run", "native_probe_mono.vl"])
        .output()
        .expect("run the probe on the JS backend");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&javascript.stdout),
        "the two backends disagree about the monomorphised program"
    );
}

/// The monomorphisation probe: one generic function, one generic struct and
/// one generic enum, each at TWO instantiations, plus a non-generic function
/// whose emitted name must not move.
const MONO_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun identity<T>(value: T): T { value }\n",
    "\n",
    "fun plain(n: i32): i32 { n + 1 }\n",
    "\n",
    "struct Pair<A, B> { left: A, right: B }\n",
    "\n",
    "enum Tree<T> { Leaf(T), Empty }\n",
    "\n",
    "fun main() {\n",
    "\tprint(identity(3));\n",
    "\tprint(identity(\"hi\"));\n",
    "\tprint(plain(1));\n",
    "\tlet a = Pair { left = 1, right = \"two\" };\n",
    "\tlet b = Pair { left = \"a\", right = 2 };\n",
    "\tprint(a.left);\n",
    "\tprint(b.right);\n",
    "\tlet leaf: Tree<i32> = Tree::Leaf(7);\n",
    "\tlet word: Tree<str> = Tree::Leaf(\"x\");\n",
    "\tmatch leaf {\n",
    "\t\tTree::Leaf(let v) => { print(v); },\n",
    "\t\tTree::Empty => { print(0); },\n",
    "\t}\n",
    "\tmatch word {\n",
    "\t\tTree::Leaf(let v) => { print(v); },\n",
    "\t\tTree::Empty => { print(\"\"); },\n",
    "\t}\n",
    "}\n",
);

/// A string literal's ESCAPES mean the same thing on both backends.
///
/// S1a wrote a literal's source text straight into a Rust literal and escaped
/// its backslashes, so `print("a\nb")` printed `a\nb` natively against the JS
/// backend's two lines — and because no program in the accepted corpus carried
/// an escape, the differential never saw it. This is that class, as a probe,
/// covering each of the six escapes vilan recognises, an unknown escape (which
/// keeps both characters), and a literal backslash.
#[test]
fn a_string_literals_escapes_mean_the_same_thing_on_both_backends() {
    let staged = stage();
    let probe = staged.join("native_probe_escapes.vl");
    std::fs::write(&probe, ESCAPE_PROBE).expect("write the probe program");
    let native = vilan(&staged)
        .args(["run", "--backend", "rust", "native_probe_escapes.vl"])
        .output()
        .expect("run the escape probe natively");
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    let javascript = vilan(&staged)
        .args(["run", "native_probe_escapes.vl"])
        .output()
        .expect("run the escape probe on the JS backend");
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        String::from_utf8_lossy(&javascript.stdout),
        "the two backends disagree about a string literal's escapes"
    );
    // Non-vacuous by construction: the JS side really does interpret them.
    assert!(
        String::from_utf8_lossy(&javascript.stdout).contains("a\nb"),
        "the probe's `\\n` must be a real newline on the JS side: {:?}",
        String::from_utf8_lossy(&javascript.stdout)
    );
}

const ESCAPE_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun main() {\n",
    "\tprint(\"a\\nb\");\n",
    "\tprint(\"tab\\there\");\n",
    "\tprint(\"cr\\rhere\");\n",
    "\tprint(\"quote\\\"q\");\n",
    "\tprint(\"back\\\\slash\");\n",
    "\tprint(\"unknown\\u0041escape\");\n",
    "}\n",
);

/// C15's measurement, as a pin rather than as a number in a report: the
/// emitter's boxed-binding count is REACHABLE, and it is zero over a program
/// with no mutably-captured binding and non-zero over one that has.
///
/// R3 ruled that v1 boxes every mutably-captured binding and that the count is
/// what pays for the by-value capture optimisation later. A measurement nothing
/// holds to a shape is a measurement that silently stops being taken.
#[test]
fn the_boxed_binding_count_is_reachable_and_counts_the_right_bindings() {
    let staged = stage();
    let none = staged.join("native_probe_boxed_none.vl");
    std::fs::write(
        &none,
        "import std::io::print;\n\nfun main() {\n\tlet n = 1;\n\tlet f = || { n + 1 };\n\tprint(f());\n}\n",
    )
    .expect("write the probe");
    let some = staged.join("native_probe_boxed_some.vl");
    std::fs::write(
        &some,
        "import std::io::print;\n\nfun main() {\n\tmut n = 1;\n\tlet bump = || { n = n + 1; };\n\tbump();\n\tbump();\n\tprint(n);\n}\n",
    )
    .expect("write the probe");

    let count = |program: &str| {
        let output = vilan(&staged)
            .env("VILAN_NATIVE_REPORT_BOXED", "1")
            .args(["build", "--backend", "rust", program])
            .output()
            .expect("build the probe");
        assert!(
            output.status.success(),
            "{program}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8_lossy(&output.stdout);
        text.lines()
            .find_map(|line| line.strip_prefix("vilan-native: boxed-bindings="))
            .unwrap_or_else(|| panic!("{program} printed no boxed-bindings line:\n{text}"))
            .trim()
            .parse::<usize>()
            .expect("a count")
    };
    assert_eq!(
        count("native_probe_boxed_none.vl"),
        0,
        "a binding a closure only READS is not boxed"
    );
    assert_eq!(
        count("native_probe_boxed_some.vl"),
        1,
        "a binding a closure WRITES is boxed (spec §6.9: a closure captures the binding)"
    );
}

/// A numeric literal in an ASSIGNMENT takes its width from the position, the
/// way one in a `let` already did (B370's law, on the paths the native emitter
/// had not carried it down).
///
/// `mut i: u53 = 5; i -= 1;` emitted `i - (1i32)` against a `u64` and rustc
/// refused the program — a BACKEND defect, and one the byte gate could not see
/// because no corpus program assigns a literal to a non-default width. Four
/// positions are in here, because each carries the expectation down a
/// different path: a plain binding, a subscript, a field, a counted cell's
/// `write()`, and a module-level binding's own initializer. The subscripts are
/// deliberately literal: an index is an index whatever the assignment expects,
/// and a first fix made `xs[1]` come out `xs[(1u64)]`.
const LITERAL_WIDTH_PROBE: &str = concat!(
    "import std::shared::Shared;\n",
    "\n",
    "struct Counter { n: u53 }\n",
    "\n",
    "mut level: u32 = 10;\n",
    "\n",
    "fun main() {\n",
    "\tmut a: u53 = 5;\n",
    "\ta -= 1;\n",
    "\ta += 2;\n",
    "\ta *= 3;\n",
    "\tmut b: i53 = 9;\n",
    "\tb = b - 1;\n",
    "\tmut c: u32 = 7;\n",
    "\tc /= 2;\n",
    "\tmut d: u8 = 200;\n",
    "\td -= 100;\n",
    "\tmut e: i8 = -5;\n",
    "\te += 3;\n",
    "\tmut f: f64 = 1.5;\n",
    "\tf *= 2;\n",
    "\tmut xs: List<u53> = [5u53, 6u53];\n",
    "\txs[0] -= 1;\n",
    "\txs[1] = xs[1] + 2;\n",
    "\tmut counter = Counter { n = 9 };\n",
    "\tcounter.n -= 4;\n",
    "\tlet cell: Shared<u53> = Shared::new(3u53);\n",
    "\tcell.write() = cell.read() + 1;\n",
    "\tlevel -= 3;\n",
    "\tprint(xs);\n",
    "\tprint(i\"{a} {b} {c} {d} {e} {f} {counter.n} {cell.read()} {level}\");\n",
    "}\n",
);

#[test]
fn a_literal_assigned_to_a_narrow_binding_takes_the_bindings_width() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_width.vl"), LITERAL_WIDTH_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_width.vl"),
        Verdict::Identical,
        "a literal assigned into a non-default width must take that width — a mismatch is a \
         rustc refusal of the emitted Rust, which is a backend defect"
    );
}

/// B389's five literal positions, natively: an unsuffixed literal takes its
/// context's width where the POSITION states none the emitter can read — the
/// left operand of a comparison and of an arithmetic operator, a `match` arm,
/// a generic call's argument, a list literal's elements, and a bare `let`
/// typed by a later use (through a comparison peer and a binding built from
/// it). The JS backend has one number and never asks; the Rust one wrote
/// `0i32 < n_u64` and `identity((5i32))` for a `u64` instance until the
/// solver recorded each literal's settled width.
const LITERAL_POSITIONS_PROBE: &str = concat!(
    "fun take(count: u53): u53 { count }\n",
    "fun half(value: f64): f64 { value / 2 }\n",
    "fun identity<T>(value: T): T { value }\n",
    "\n",
    "fun main() {\n",
    "\tlet n: u53 = 4;\n",
    "\tprint(take(1 + n));\n",
    "\tif 0 < n { print(\"positive\"); }\n",
    "\tlet m = 10 - n;\n",
    "\tprint(take(m));\n",
    "\tlet xs: List<u32> = [0, 1, 2];\n",
    "\tprint(xs[2]);\n",
    "\tmatch n {\n",
    "\t\t4 => print(\"four\"),\n",
    "\t\t_ => print(\"other\"),\n",
    "\t}\n",
    "\tlet k: i53 = identity(5);\n",
    "\tprint(k);\n",
    "\tlet bare = 7;\n",
    "\tprint(take(bare));\n",
    "\tmut i = 0;\n",
    "\tlet limit: u53 = 3;\n",
    "\tfor i < limit { i += 1; }\n",
    "\tprint(take(i));\n",
    "\tlet one = 1;\n",
    "\tlet halved = one / 2;\n",
    "\tprint(half(one));\n",
    "\tprint(halved);\n",
    "\tlet x: f64 = 3;\n",
    "\tprint(1 / x);\n",
    "\tprint(3 / 2.0);\n",
    "\tlet quarter = 1 / 4.0;\n",
    "\tprint(quarter);\n",
    "}\n",
);

#[test]
fn the_five_literal_positions_take_their_contexts_width_natively() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_literal_positions.vl"),
        LITERAL_POSITIONS_PROBE,
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_literal_positions.vl"),
        Verdict::Identical,
        "a literal must take its context's width in every position B389 names — a mismatch is \
         a rustc refusal of the emitted Rust, which is a backend defect"
    );
}

/// B397: `combine` over a source whose value is itself a TUPLE. The JS
/// backend read it wrong (`x=1,2 y=c l=undefined`) until the comprehension was
/// emitted unrolled for that instance; the native one refuses a mapped tuple by
/// name today. The claim held here is the differential's own: whatever the
/// native backend does with this program, it is never a DIFFERENT answer — a
/// refusal now, the same bytes once it lowers comprehensions.
const COMBINE_TUPLE_ELEMENT_PROBE: &str = concat!(
    "import std::reactive::{ SignalCell, combine };\n",
    "\n",
    "fun main() {\n",
    "\tlet point = SignalCell::new((1, 2));\n",
    "\tlet label = SignalCell::new(\"c\");\n",
    "\tlet both = combine((point, label)).memo();\n",
    "\tlet ((x, y), l) = both.get();\n",
    "\tprint(i\"{x} {y} {l}\");\n",
    "\tpoint.set((3, 4));\n",
    "\tlet ((x2, y2), l2) = both.get();\n",
    "\tprint(i\"{x2} {y2} {l2}\");\n",
    "}\n",
);

#[test]
fn combine_over_a_tuple_valued_source_is_never_a_different_answer_natively() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_combine_tuple.vl"),
        COMBINE_TUPLE_ELEMENT_PROBE,
    )
    .expect("write the probe program");
    let verdict = compare(&staged, "native_probe_combine_tuple.vl");
    assert!(
        !matches!(verdict, Verdict::Broken(_)),
        "the native backend must refuse this program by name or print what node prints: \
         {verdict:?}"
    );
}

/// F31's trap, in one program: the read whose binding was declared OUTSIDE the
/// loop keeps its copy, and the read whose binding the loop body itself
/// declares moves.
///
/// "No later read" alone gets the first one wrong — `weigh(names)` IS the last
/// read of `names` in the text, and moving there empties the binding the second
/// iteration reads. Nothing reads `names` after the loop, deliberately: a read
/// after it would make the loop read not-last for the trivial reason and the
/// pin would measure nothing. The rule asks "deeper in a repeating region than
/// the DECLARATION", which answers both halves with one question, and the two
/// are in one program so a fix that satisfies either alone fails here.
const LOOP_TRAP_PROBE: &str = concat!(
    "struct Row { id: usize, tags: List<str> }\n",
    "\n",
    "fun weigh(tags: List<str>): usize { tags.len() }\n",
    "fun weigh_row(row: Row): usize { row.tags.len() + row.id }\n",
    "\n",
    "fun main() {\n",
    "\tlet names = [\"alpha\", \"beta\"];\n",
    "\tmut total = 0;\n",
    "\tmut i = 0;\n",
    "\tfor i < 3 {\n",
    "\t\ttotal = total + weigh(names);\n",
    "\t\tlet row = Row { id = i, tags = [\"one\"] };\n",
    "\t\ttotal = total + weigh_row(row);\n",
    "\t\ti = i + 1;\n",
    "\t}\n",
    "\tprint(total);\n",
    "}\n",
);

#[test]
fn a_last_use_inside_a_loop_keeps_its_copy_and_one_declared_inside_it_moves() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_loop_trap.vl"), LOOP_TRAP_PROBE)
        .expect("write the probe program");
    assert_eq!(
        copy_census_of(&staged, "native_probe_loop_trap.vl"),
        (1, 1),
        "the read of a binding declared OUTSIDE the loop must copy, and the read of one the \
         loop body declares must move"
    );
    assert_eq!(
        compare(&staged, "native_probe_loop_trap.vl"),
        Verdict::Identical,
        "and the program still prints the same bytes on both backends"
    );
}

/// F31's census: the consumed place reads the native emitter COPIES, and the
/// ones it moves because the read is its binding's last use.
///
/// The committed table, beside `copy-elision-census.tsv` and for the same
/// reason its own head gives: the byte gate above already holds every one of
/// these programs identical on both backends, so a change in elision cannot
/// ship unnoticed — but it arrives as an unlabelled behaviour, and elision is
/// exactly where the gate and the meaning come apart. A program that loses a
/// copy is a win; a program that GAINS one is a regression in the liveness
/// walk, and byte-identical output says the same thing about both.
///
/// **What is counted** is not every `.clone()` in the emitted Rust — a refcount
/// bump on a handle is one of those and is not a copy. It is the copies
/// `copy_a_consumed_place_read` decides: the consumed positions rule 1's own
/// marking never reached (a closure call's arguments, a variant constructor's,
/// a destructure's), which are exactly the ones the native liveness pass is
/// answerable for.
///
/// **The programs** are [`DEFAULT_SUITE`] plus the paper's board probe, because
/// that is the set the byte gate runs on every build; the whole corpus is one
/// list away and costs an emit per program.
const NATIVE_COPY_CENSUS: &str = "crates/vilan-cli/tests/native-copy-census.tsv";

/// One program's census line, as the compiler reports it under
/// `VILAN_NATIVE_REPORT_COPIES=1`.
fn copy_census_of(staged: &Path, program: &str) -> (usize, usize) {
    let output = vilan(staged)
        .env("VILAN_NATIVE_REPORT_COPIES", "1")
        .args(["build", "--backend", "rust", "--stdout", program])
        .output()
        .expect("emit the program");
    assert!(
        output.status.success(),
        "{program} must emit for the census:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find(|line| line.starts_with("vilan-native: consumed-copies="))
        .unwrap_or_else(|| panic!("{program} reported no copy census"));
    let mut numbers = line
        .split(|character: char| !character.is_ascii_digit())
        .filter(|piece| !piece.is_empty())
        .map(|piece| piece.parse::<usize>().expect("a count"));
    let copied = numbers.next().expect("the copied count");
    let elided = numbers.next().expect("the elided count");
    (copied, elided)
}

#[test]
fn the_native_copy_census_matches_its_table() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_board.vl"), BOARD_PROBE)
        .expect("write the board probe");
    let mut rows = Vec::new();
    for program in DEFAULT_SUITE
        .iter()
        .copied()
        .chain(std::iter::once("native_probe_board.vl"))
    {
        let (copied, elided) = copy_census_of(&staged, program);
        rows.push(format!(
            "{}\t{copied}\t{elided}",
            program.trim_end_matches(".vl")
        ));
    }
    let measured = format!(
        "{}{}\n",
        concat!(
            "# Consumed place reads the NATIVE emitter copied, and the ones it\n",
            "# moved at a last use (tracker F31). Regenerate with\n",
            "# VILAN_REGENERATE_NATIVE_COPY_CENSUS=1 cargo test -p vilan-cli \
             --test native_differential\n",
        ),
        rows.join("\n")
    );
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(NATIVE_COPY_CENSUS);
    if std::env::var_os("VILAN_REGENERATE_NATIVE_COPY_CENSUS").is_some() {
        std::fs::write(&path, &measured).expect("write the census");
        return;
    }
    let committed = std::fs::read_to_string(&path).expect("read the committed census");
    assert_eq!(
        committed, measured,
        "the native copy census moved; read the difference, then regenerate with \
         VILAN_REGENERATE_NATIVE_COPY_CENSUS=1"
    );
}

/// F29: a program whose host bindings sit ONLY inside a refused call — in its
/// arguments, and in the body of a closure among them.
///
/// `host_name` is reached nowhere but inside the argument of the refused
/// `set_priority` call; `range_i32` nowhere but inside the body of the closure
/// the refused `hmr_register_teardown` takes. Neither was in the census before
/// the walk continued past a refusal, which is how `createServer`'s handler hid
/// nine bindings from the census that sized F18.
///
/// The argument pair is the PROGRAM's own `node:os` bindings, which no backend
/// table will ever answer: the pair had been `random_bytes(random_uuid()..)`,
/// and F40 lowered both, which turned the pin's shape into a program with
/// nothing hidden in it. A pair the runtime cannot grow into keeps it measuring
/// the walk rather than the runtime's coverage.
const CENSUS_PROBE: &str = concat!(
    "import std::random::range_i32;\n",
    "import std::rpc::hmr_register_teardown;\n",
    "import std::io::print;\n",
    "\n",
    "[extern(\"node:os\", \"hostname\")]\n",
    "external fun host_name(): str;\n",
    "\n",
    "[extern(\"node:os\", \"setPriority\")]\n",
    "external fun set_priority(name: str): i32;\n",
    "\n",
    "fun main() {\n",
    "\tprint(set_priority(host_name()));\n",
    "\thmr_register_teardown(|| { print(range_i32(1, 4)); });\n",
    "}\n",
);

/// F29's pin: the host census reports what a REFUSAL stands in front of.
///
/// The census answers "what host surface does this program still need", and a
/// walk that stops at the first refusal answers a smaller question. The two
/// bindings asserted here are each reachable through exactly ONE refused
/// construct, so a walk that stops names neither — which is what makes this
/// pin measure the continuation rather than the program.
#[test]
fn the_host_census_walks_past_a_refusal_into_its_arguments_and_closure_bodies() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_census.vl"), CENSUS_PROBE)
        .expect("write the probe program");
    let output = vilan(&staged)
        .env("VILAN_NATIVE_HOST_CENSUS", "1")
        .args([
            "build",
            "--backend",
            "rust",
            "--stdout",
            "native_probe_census.vl",
        ])
        .output()
        .expect("census the probe");
    assert!(
        output.status.success(),
        "the census emit failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let census = String::from_utf8_lossy(&output.stdout);
    for hidden in [
        // Reached only inside the ARGUMENT of a refused host binding's call.
        "the host binding `host_name`",
        // Reached only inside the BODY of a closure handed to a refused one.
        "the intrinsic `RandomInt`",
    ] {
        assert!(
            census.contains(hidden),
            "the census must walk past a refusal and report `{hidden}`:\n{census}"
        );
    }
    // And the refusals themselves are still reported, which is what the walk is
    // a continuation OF.
    for refused in [
        "the host binding `set_priority`",
        "the host binding `hmr_register_teardown`",
    ] {
        assert!(
            census.contains(refused),
            "the census must still name the refusal itself `{refused}`:\n{census}"
        );
    }
}

/// F30: a binding that lives in a CELL, mutated IN PLACE through every spelling
/// the language has for it.
///
/// A module-level binding is a `thread_local!` and a mutably-captured one is a
/// `Captured` cell; a read of either answers a VALUE, which is right for a value
/// and silently wrong for a place — the mutation lands in a temporary that is
/// dropped at the end of the statement. Every line below printed the
/// UNMUTATED value natively while the JS backend printed the mutated one, and
/// none of them was visible to the whole-set differential because every mutated
/// module binding in the corpus holds a `Shared`, whose copy is the same cell.
///
/// The last two lines are the borrow's own hazard rather than the copy's: the
/// cell is borrowed for the whole of the mutating call, so a read of the same
/// binding among the ARGUMENTS has to happen before the borrow is taken.
const CELL_PLACE_PROBE: &str = concat!(
    "struct Counter { n: i32 }\n",
    "\n",
    "impl Counter {\n",
    "\tfun bump(&mut self) { self.n = self.n + 1; }\n",
    "}\n",
    "\n",
    "mut counts: List<usize> = [1, 2];\n",
    "mut counter: Counter = Counter { n = 0 };\n",
    "\n",
    "fun record(value: usize) { counts.push(value); }\n",
    "\n",
    "fun grow(xs: &mut List<usize>, by: usize) { xs.push(by); }\n",
    "\n",
    "fun main() {\n",
    "\trecord(7);\n",
    "\tprint(counts);\n",
    "\tcounter.bump();\n",
    "\tcounter.bump();\n",
    "\tprint(counter.n);\n",
    "\tcounter.n = 41;\n",
    "\tprint(counter.n);\n",
    "\tcounts[0] = 5;\n",
    "\tprint(counts);\n",
    "\tgrow(&mut counts, 9);\n",
    "\tprint(counts);\n",
    "\tmut seen: List<usize> = [];\n",
    "\tmut inner: Counter = Counter { n = 0 };\n",
    "\tlet bump = || { seen.push(seen.len()); inner.bump(); };\n",
    "\tbump();\n",
    "\tbump();\n",
    "\tprint(seen);\n",
    "\tprint(inner.n);\n",
    "\tcounts.push(counts.len());\n",
    "\tprint(counts);\n",
    "\tgrow(&mut counts, counts.len());\n",
    "\tprint(counts);\n",
    "}\n",
);

/// F30's pin: every cell-resident binding above is mutated IN PLACE, on both
/// backends, to the same bytes.
///
/// It is written as ONE program because it is one defect seen from seven
/// sides — a mutating intrinsic's receiver, a `&mut self` method, a field
/// write, an index write, a `&mut` argument, and the two argument orders the
/// borrow constrains — and because a program that mixes them is the one that
/// catches a fix applied at only one of them.
#[test]
fn a_binding_that_lives_in_a_cell_is_mutated_in_place_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_cell_place.vl"), CELL_PLACE_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_cell_place.vl"),
        Verdict::Identical,
        "a module-level or mutably-captured binding mutated in place must be mutated in the CELL, \
         not in a copy of its value"
    );
}

/// `native-apps.md`'s probe, verbatim from
/// `proposals/projects/vilan/proposal/native-apps-probe/board.vl` — copied
/// rather than referenced because the proposals repository is not a build
/// input.
const BOARD_PROBE: &str = concat!(
    "import std::reactive::{ Owner, SignalCell };\n",
    "\n",
    "struct Todo { id: i32, title: str, done: bool }\n",
    "\n",
    "struct Board { name: str, todos: List<Todo> }\n",
    "\n",
    "impl Board {\n",
    "\tfun new(name: str): Board { Board { name, todos = [] } }\n",
    "\tfun add(&mut self, todo: Todo) { self.todos.push(todo); }\n",
    "\tfun open(self): i32 {\n",
    "\t\tmut n = 0;\n",
    "\t\tfor todo in self.todos { if !todo.done { n = n + 1; } }\n",
    "\t\tn\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tmut board = Board::new(\"inbox\");\n",
    "\tboard.add(Todo { id = 1, title = \"write the paper\", done = false });\n",
    "\tboard.add(Todo { id = 2, title = \"read the census\", done = true });\n",
    "\tmut snapshot = board.todos;\n",
    "\tsnapshot.push(Todo { id = 3, title = \"ghost\", done = false });\n",
    "\tprint(board.todos.len());\n",
    "\tprint(snapshot.len());\n",
    "\tlet count: SignalCell<i32> = SignalCell::new(board.open());\n",
    "\tmut seen: List<i32> = [];\n",
    "\tlet owner = Owner::new();\n",
    "\tlet _sub = owner.take(count.sub(|value| { seen.push(value); }));\n",
    "\tboard.add(Todo { id = 4, title = \"ship it\", done = false });\n",
    "\tcount.set(board.open());\n",
    "\tprint(seen.len());\n",
    "\towner.dispose();\n",
    "\tcount.set(99);\n",
    "\tprint(seen.len());\n",
    "\tprint(count.get());\n",
    "}\n",
);

/// J6's exit: every async corpus program the backend ACCEPTS prints
/// byte-identically, and the ones it refuses say which construct stopped them.
///
/// The census is printed for the same reason the corpus sweep prints its own —
/// the list of refusals IS the work list, and a reader who runs this wants to
/// see it shrink.
#[test]
fn every_async_corpus_program_is_identical_or_named() {
    let staged = stage();
    let mut identical_programs: Vec<String> = Vec::new();
    let mut refused: Vec<(String, String)> = Vec::new();
    let mut broken = Vec::new();
    for program in ASYNC_SUITE {
        match compare(&staged, program) {
            Verdict::Identical => identical_programs.push((*program).to_string()),
            Verdict::Refused(reason) => refused.push(((*program).to_string(), reason)),
            Verdict::Broken(detail) => broken.push(format!("{program}: {detail}")),
        }
    }
    eprintln!(
        "async differential: {} enumerated, {} identical, {} refused by name, {} broken",
        ASYNC_SUITE.len(),
        identical_programs.len(),
        refused.len(),
        broken.len()
    );
    for (program, reason) in &refused {
        eprintln!("  refused  {program}: {reason}");
    }
    for program in &identical_programs {
        eprintln!("  identical  {program}");
    }
    assert!(
        broken.is_empty(),
        "async programs the native backend ACCEPTED and then got wrong:\n{}",
        broken.join("\n")
    );
    // The two that compile today, and they are the right two.
    // `await-postfix.vl` was written as a BYTE-level gate on where the
    // parentheses of an await go, and every helper in it awaits, so every call
    // to one is awaited on the caller's behalf. `nursery.vl` is structured
    // concurrency whole: a helper's spawn, a grandchild spawned by a running
    // child, and a join that must wait for a child list which GREW while it was
    // draining. If either stops being identical the executor or the emitter's
    // async arms have moved.
    // F22: `adapt.vl` joins them. It is the corpus's ADAPTED-INSTANCE program
    // — one `map` called with an async closure at one site and a synchronous
    // one at another, and a `run` the same way — so it is the pin that this
    // emitter monomorphises on asyncness as well as on types. Two instances of
    // each callee come out, one `async fn` and one plain, and the program that
    // proves it is the one whose two answers must be the same bytes.
    for required in ["await-postfix.vl", "nursery.vl", "adapt.vl"] {
        assert!(
            identical_programs.iter().any(|program| program == required),
            "{required} must be byte-identical on both backends; the census was:\n{}",
            refused
                .iter()
                .map(|(program, reason)| format!("  {program}: {reason}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// The executor's ORDERING, through an emitted program rather than through a
/// `vilan-rt` unit test (J6).
///
/// `reactive-turns.vl` is the corpus's ordering pin and it is still refused for
/// its generics, so the ordering the backend must reproduce is pinned here
/// instead, on the three rules a reader can check by eye: a spawn is EAGER (its
/// `enter` prints before the line after the spawn expression), the deadline list
/// is ordered (the 5 ms task finishes before the 500 ms one that was spawned
/// FIRST), and a join answers the spawned value.
///
/// The two deadlines are 5 ms and 500 ms because this probe's claim is an
/// ORDER, and the suite's rule for one is at the head (N116): a hundredfold
/// gap, so no scheduling stall on this machine can let the later deadline tie
/// with the earlier and invert the two runtimes' answers.
#[test]
fn the_spawn_and_sleep_ordering_is_byte_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_spawn.vl"), SPAWN_ORDER_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_spawn.vl"),
        Verdict::Identical,
        "the executor's spawn and sleep ordering must match the JS turn model"
    );
    // N116: and the order the two agree on is the order this probe CLAIMS.
    //
    // `compare` asks only whether the two backends print the same bytes, so a
    // change that inverted the deadline list on BOTH of them would satisfy it
    // perfectly — and, before the deadlines were widened, a busy box could
    // invert node's alone, which is what made this pin flaky. Stating the
    // expected stdout costs one more `vilan run` and turns both of those into
    // a red that names the rule.
    let javascript = vilan(&staged)
        .args(["run", "native_probe_spawn.vl"])
        .output()
        .expect("run the JS backend");
    assert!(javascript.status.success(), "{javascript:?}");
    assert_eq!(
        String::from_utf8_lossy(&javascript.stdout),
        // A spawn is EAGER, so both `enter` lines precede `spawned`; the
        // deadline list is ordered, so `early` leaves first although `late`
        // was spawned first; and each join answers its own task's value.
        "enter late\nenter early\nspawned\nleave early\nearly\nleave late\nlate\n",
        "the three rules this probe exists for, spelled out"
    );
}

/// `std::time::Timer`'s memoized verdict, through an emitted program: a fired
/// timer answers `true` twice (from the memo, not from a second timer) and a
/// cancelled one answers `false`.
#[test]
fn a_timers_verdict_is_byte_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_timer.vl"), TIMER_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_timer.vl"),
        Verdict::Identical,
        "`Timer`'s verdict must be the same on both backends"
    );
}

/// `Task::settle_all` and `Task::race` — the two joins on `Task<T>`, through an
/// emitted program (J6).
///
/// `async-promise-all.vl` is the corpus's own pin on `settle_all` and it is
/// still refused, for its `[extern("node:timers/promises", "setTimeout")]`
/// rather than for anything about the join, so the same shape is pinned here
/// over `std::time::sleep`. `settle_all` preserves ORDER whatever the delays
/// are, and `race` answers the first task to settle while its loser keeps
/// running.
#[test]
fn the_two_task_joins_are_byte_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_joins.vl"), TASK_JOIN_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_joins.vl"),
        Verdict::Identical,
        "`Task::settle_all` and `Task::race` must answer the same on both backends"
    );
}

const TASK_JOIN_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::task::Task;\n",
    "import std::time::sleep;\n",
    "\n",
    "fun delayed(label: str, ms: i32): str {\n",
    "\tsleep(ms);\n",
    "\tlabel\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tmut tasks: List<Task<str>> = List::new();\n",
    "\ttasks.push(async delayed(\"a\", 20));\n",
    "\ttasks.push(async delayed(\"b\", 10));\n",
    "\ttasks.push(async delayed(\"c\", 30));\n",
    "\tlet results: List<str> = Task::settle_all(tasks);\n",
    "\tfor result in results {\n",
    "\t\tprint(result);\n",
    "\t}\n",
    "\tmut racers: List<Task<str>> = List::new();\n",
    "\tracers.push(async delayed(\"slow\", 40));\n",
    "\tracers.push(async delayed(\"quick\", 5));\n",
    "\tprint(Task::race(racers));\n",
    "}\n",
);

const SPAWN_ORDER_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::time::sleep;\n",
    "\n",
    "async fun step(label: str, ms: i32): str {\n",
    "\tprint(i\"enter {label}\");\n",
    "\tsleep(ms);\n",
    "\tprint(i\"leave {label}\");\n",
    "\tlabel\n",
    "}\n",
    "\n",
    "fun main() {\n",
    // N116: 1 ms and 12 ms were the deadlines, and under a parallel lane both
    // could be overdue by the time either runtime reached its timer phase —
    // at which point node fires them in INSERTION order (`late` first) while
    // the native executor fires them in DEADLINE order, and the two stdouts
    // part company over nothing. 5 ms and 500 ms is the same claim with a
    // stall budget: the ordering inverts only if a runtime takes half a
    // second to look at its timers.
    "\tlet late = async step(\"late\", 500);\n",
    "\tlet early = async step(\"early\", 5);\n",
    "\tprint(\"spawned\");\n",
    "\tprint(await early);\n",
    "\tprint(await late);\n",
    "}\n",
);

const TIMER_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::time::Timer;\n",
    "\n",
    "fun main() {\n",
    "\tlet fired = Timer::after(1);\n",
    "\tprint(fired.wait());\n",
    "\tprint(fired.wait());\n",
    "\tlet called_off = Timer::after(500);\n",
    "\tcalled_off.cancel();\n",
    "\tprint(called_off.wait());\n",
    "\tprint(\"done\");\n",
    "}\n",
);

#[test]
fn the_enumeration_finds_a_corpus_and_excludes_the_platform_programs() {
    let programs = platform_free_programs();
    assert!(
        programs.len() > 80,
        "the platform-free enumeration found only {} programs, which means the corpus moved or \
         the filter over-reaches",
        programs.len()
    );
    assert!(
        !programs
            .iter()
            .any(|program| program == "db.vl" || program == "file.vl"),
        "a program reaching the filesystem or a database is not platform-free"
    );
    assert!(programs.iter().any(|program| program == "bool.vl"));
}

/// F56: a `[resource]` type with NO `Drop` impl needs no teardown — move-only
/// is the analyzer's, and it reads the same on both backends — so the native
/// build emits it as an ordinary type. A142's pipe nodes are exactly that
/// (Drop-less `[resource]` structs holding closures), and before F56 every
/// program deriving a signal was refused at its first node.
///
/// The cases are Appendix A's prototype (`native/pipe_prototype.vl`): a fused
/// chain sealed once, a counting `switch` selector, a root and a pipe handed to
/// an `own` parameter, two chains sealed in sequence, and a pipe chosen by a
/// branch beside a Drop-less resource ENUM carrying one.
#[test]
fn a_resource_without_drop_builds_the_same_on_both_backends() {
    let staged = stage();
    let library = include_str!("native/pipe_prototype.vl");
    for (name, main) in [
        ("fused", PIPE_FUSED_MAIN),
        ("switch", PIPE_SWITCH_MAIN),
        ("own_parameter", PIPE_OWN_PARAMETER_MAIN),
        ("sealed_twice", PIPE_SEALED_TWICE_MAIN),
        ("branch", PIPE_BRANCH_MAIN),
    ] {
        let program = format!("native_probe_pipe_{name}.vl");
        std::fs::write(staged.join(&program), format!("{library}\n{main}"))
            .expect("write the probe program");
        assert_eq!(
            compare(&staged, &program),
            Verdict::Identical,
            "{program}: a Drop-less resource must build and print the same on both backends"
        );
    }
}

/// F56: a Drop-less resource handed on is MOVED natively, never copied. The
/// source binding is dead (the analyzer's move checker says so on both
/// backends), so a copy would be harmless for a type with no teardown — and
/// wasted, and a second owner of what the language says has one. Two move
/// sites, each of which copied before:
///
/// - a generic field read at a resource instantiation: the outer `Map`'s
///   `start` hands `self.up` — itself a `Map` — to the inner `start`. The
///   analyzer's decision is per instantiation (`UnlessResource`), and this
///   backend read only that a decision EXISTED;
/// - a destructuring `match` over an owned resource parameter, which copied
///   its subject as it does a data place's.
#[test]
fn a_resource_is_moved_not_copied_at_its_move_sites_natively() {
    let staged = stage();
    let library = include_str!("native/pipe_prototype.vl");
    let emit = |program: &str, main: &str| -> String {
        std::fs::write(staged.join(program), format!("{library}\n{main}"))
            .expect("write the probe program");
        let emitted = vilan(&staged)
            .args(["build", "--backend", "rust", "--stdout", program])
            .output()
            .expect("build the probe");
        assert!(
            emitted.status.success(),
            "{program} was refused:\n{}",
            String::from_utf8_lossy(&emitted.stderr)
        );
        String::from_utf8_lossy(&emitted.stdout).into_owned()
    };
    // The function whose signature names a parameter of a Rust type minted
    // from `prefix`, as (its parameter's name, its body).
    let owners_of = |source: &str, prefix: &str, name_prefix: &str| -> Vec<(String, String)> {
        source
            .split("\nfn ")
            .skip(1)
            .filter_map(|function| {
                let signature = function.lines().next()?;
                let marker = format!(": {prefix}");
                let at = signature.find(&marker)?;
                let name = signature[..at].rsplit(['(', ' ']).next()?.to_string();
                name.starts_with(name_prefix)
                    .then(|| (name, function.to_string()))
            })
            .collect()
    };

    // The fused chain: `Map<Map<Cell, ..>, ..>`. The Rust struct whose `up`
    // is itself a `Map` is the outer node.
    let fused = emit("native_probe_pipe_moves_fused.vl", PIPE_FUSED_MAIN);
    let outer = fused
        .split("\nstruct ")
        .skip(1)
        .find(|declaration| {
            declaration
                .lines()
                .nth(1)
                .is_some_and(|field| field.contains("up: Map_"))
        })
        .and_then(|declaration| declaration.split_whitespace().next())
        .unwrap_or_else(|| panic!("no `Map` over a `Map` in the emitted source:\n{fused}"))
        .to_string();
    let starts = owners_of(&fused, &outer, "this");
    assert!(
        !starts.is_empty(),
        "no function takes the outer node `{outer}` by value:\n{fused}"
    );
    for (_, body) in &starts {
        assert!(
            !body.contains("(this.up).clone()"),
            "the outer node's `up` (a resource) is copied where it is moved:\n{body}"
        );
    }

    // A destructuring `match` over an owned resource parameter.
    let branch = emit("native_probe_pipe_moves_branch.vl", PIPE_BRANCH_MAIN);
    let runs = owners_of(&branch, "Stage_", "s_");
    assert!(
        !runs.is_empty(),
        "no function takes a `Stage` by value:\n{branch}"
    );
    for (name, body) in &runs {
        assert!(
            body.contains(&format!("match {name} {{")),
            "the owned resource `{name}` is copied into its `match`:\n{body}"
        );
    }
}

/// F56's other half: a resource WITH a `Drop` impl is still refused by name —
/// its teardown is F1's later slice — as a struct and as an enum.
#[test]
fn a_resource_with_drop_is_still_refused_by_name_natively() {
    let staged = stage();
    for (program, source, named) in [
        (
            "native_probe_drop_struct.vl",
            DROP_STRUCT_PROBE,
            "the `resource` type `Guard`",
        ),
        (
            "native_probe_drop_enum.vl",
            DROP_ENUM_PROBE,
            "the `resource` enum `Slot`",
        ),
    ] {
        std::fs::write(staged.join(program), source).expect("write the probe program");
        match compare(&staged, program) {
            Verdict::Refused(reason) => assert!(
                reason.contains(named),
                "{program} refused for another reason: {reason}"
            ),
            other => panic!("{program}: expected a refusal by name, got {other:?}"),
        }
    }
}

const PIPE_FUSED_MAIN: &str = concat!(
    "fun main() {\n",
    "\tlet runs = Shared::new(0);\n",
    "\tlet c = Cell::new(1);\n",
    "\tlet m = c\n",
    "\t\t.derive(|x| {\n",
    "\t\t\truns.write() = runs.read() + 1;\n",
    "\t\t\tx * 2\n",
    "\t\t})\n",
    "\t\t.derive(|x| x + 1)\n",
    "\t\t.memo();\n",
    "\tprint(i\"m={m.get()} runs={runs.read()}\");\n",
    "\tc.set(5);\n",
    "\tprint(i\"m={m.get()} {m.get()} {m.get()} runs={runs.read()}\");\n",
    "}\n",
);

const PIPE_SWITCH_MAIN: &str = concat!(
    "fun main() {\n",
    "\tlet made = Shared::new(0);\n",
    "\tlet flag = Cell::new(true);\n",
    "\tlet count = Cell::new(1);\n",
    "\tlet m = flag\n",
    "\t\t.switch(|on| {\n",
    "\t\t\tmade.write() = made.read() + 1;\n",
    "\t\t\tcount.derive(|x| if on { x * 100 } else { 0 - x })\n",
    "\t\t})\n",
    "\t\t.memo();\n",
    "\tprint(i\"m={m.get()} made={made.read()}\");\n",
    "\tcount.set(2);\n",
    "\tprint(i\"m={m.get()} {m.get()} made={made.read()}\");\n",
    "\tflag.set(false);\n",
    "\tcount.set(3);\n",
    "\tprint(i\"m={m.get()} made={made.read()}\");\n",
    "}\n",
);

const PIPE_OWN_PARAMETER_MAIN: &str = concat!(
    "fun show(label: str, own x: Up<i32>) {\n",
    "\tx.start(|v| print(i\"{label}: {v}\"));\n",
    "}\n",
    "fun main() {\n",
    "\tlet c = Cell::new(1);\n",
    "\tshow(\"root\", c);\n",
    "\tshow(\"root again\", c);\n",
    "\tshow(\"pipe\", c.derive(|x| x * 10));\n",
    "\tc.set(2);\n",
    "}\n",
);

const PIPE_SEALED_TWICE_MAIN: &str = concat!(
    "fun main() {\n",
    "\tlet c = Cell::new(1);\n",
    "\tlet m = c.derive(|x| x * 2).derive(|x| x + 1).memo();\n",
    "\tc.set(5);\n",
    "\tprint(i\"{m.get()}\");\n",
    "\tlet p = c.derive(|x| x * 3);\n",
    "\tlet q = p.derive(|x| x + 1);\n",
    "\tlet n = q.memo();\n",
    "\tprint(i\"{n.get()}\");\n",
    "}\n",
);

const PIPE_BRANCH_MAIN: &str = concat!(
    "[resource]\n",
    "enum Stage {\n",
    "\tDoubled(Map<Cell<i32>, i32, i32>),\n",
    "\tPlain(Cell<i32>),\n",
    "}\n",
    "fun stage(on: bool, c: Cell<i32>): Stage {\n",
    "\tif on {\n",
    "\t\tStage::Doubled(c.derive(|x| x * 2))\n",
    "\t} else {\n",
    "\t\tStage::Plain(c)\n",
    "\t}\n",
    "}\n",
    "fun run(label: str, own s: Stage) {\n",
    "\tmatch s {\n",
    "\t\tStage::Doubled(let p) => p.start(|v| print(i\"{label} doubled {v}\")),\n",
    "\t\tStage::Plain(let c) => c.start(|v| print(i\"{label} plain {v}\")),\n",
    "\t}\n",
    "}\n",
    "fun main() {\n",
    "\tlet c = Cell::new(1);\n",
    "\tlet p = if c.get() > 0 { c.derive(|x| x + 1) } else { c.derive(|x| x - 1) };\n",
    "\tlet m = p.memo();\n",
    "\trun(\"a\", stage(true, c));\n",
    "\trun(\"b\", stage(false, c));\n",
    "\tc.set(7);\n",
    "\tprint(i\"m={m.get()}\");\n",
    "}\n",
);

const DROP_STRUCT_PROBE: &str = concat!(
    "import std::drop::Drop;\n",
    "[resource]\n",
    "struct Guard {\n",
    "\tname: str,\n",
    "}\n",
    "impl Guard with Drop {\n",
    "\tfun drop(&mut self) {\n",
    "\t\tprint(i\"closing {self.name}\");\n",
    "\t}\n",
    "}\n",
    "fun main() {\n",
    "\tlet g = Guard { name = \"a\" };\n",
    "\tprint(g.name);\n",
    "}\n",
);

const DROP_ENUM_PROBE: &str = concat!(
    "import std::drop::Drop;\n",
    "[resource]\n",
    "enum Slot {\n",
    "\tFull(str),\n",
    "\tEmpty,\n",
    "}\n",
    "impl Slot with Drop {\n",
    "\tfun drop(&mut self) {\n",
    "\t\tprint(\"closing\");\n",
    "\t}\n",
    "}\n",
    "fun main() {\n",
    "\tlet s = Slot::Full(\"a\");\n",
    "\tmatch s {\n",
    "\t\tSlot::Full(let name) => print(name),\n",
    "\t\tSlot::Empty => print(\"empty\"),\n",
    "\t}\n",
    "}\n",
);

/// F53: a closure whose BODY is erased to `dyn` — `roots.map(|r| r)` into a
/// `List<dyn Src>`, the very spelling B435's steer gives — wraps its tail
/// natively as the JS emitter pairs it. The expression-bodied closure's tail
/// was rendered without the erasure every other value position applies, and
/// rustc refused the closure (`expected Dyn<..>, found Root`). Beside it: a
/// block-bodied twin, a closure-typed binding and parameter answering `dyn`,
/// whose CALL is then a receiver (it had no recorded type, so the slot call
/// through it was refused by name), and the `Some(r)` shape that always worked.
#[test]
fn a_closure_body_erased_to_an_object_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_f53.vl"), F53_PROBE).expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_f53.vl"),
        Verdict::Identical,
        "a closure body erased to `dyn` must build and print the same on both backends"
    );
}

const F53_PROBE: &str = concat!(
    "trait Src {\n",
    "\tfun get(self): i32;\n",
    "}\n",
    "struct Root {\n",
    "\tn: i32,\n",
    "}\n",
    "impl Root with Src {\n",
    "\tfun get(self): i32 {\n",
    "\t\tself.n\n",
    "\t}\n",
    "}\n",
    "fun apply(f: |Root| dyn Src, r: Root): i32 {\n",
    "\tf(r).get()\n",
    "}\n",
    "fun main() {\n",
    "\tlet roots: List<Root> = [Root { n = 1 }, Root { n = 2 }];\n",
    "\tlet ys: List<dyn Src> = roots.map(|r| r);\n",
    "\tprint(ys[1].get());\n",
    "\tlet zs: List<dyn Src> = roots.map(|r| {\n",
    "\t\tlet doubled = Root { n = r.n * 2 };\n",
    "\t\tdoubled\n",
    "\t});\n",
    "\tprint(zs[1].get());\n",
    "\tlet erase: |Root| dyn Src = |r| r;\n",
    "\tprint(erase(Root { n = 5 }).get());\n",
    "\tprint(apply(|r| r, Root { n = 7 }));\n",
    "\tlet r = Root { n = 3 };\n",
    "\tlet o: Option<dyn Src> = Some(r);\n",
    "\tmatch o {\n",
    "\t\tSome(let s) => print(s.get()),\n",
    "\t\tNone => print(0),\n",
    "\t}\n",
    "}\n",
);

/// A142 parity (reactive-44's `Switch` node): a FIELD read off a `Shared`'s
/// read — `(followed.read().pull)()`, a node calling its current inner
/// instance's closure — builds natively. The read is an intrinsic call with no
/// recorded type of its own, so the field read had no struct to name its field
/// from and was refused by name; its type is the cell's element.
#[test]
fn a_field_read_off_a_shared_read_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_shared_field.vl"),
        SHARED_FIELD_PROBE,
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_shared_field.vl"),
        Verdict::Identical,
        "a field read off a `Shared` read must build and print the same on both backends"
    );
}

const SHARED_FIELD_PROBE: &str = concat!(
    "import std::shared::Shared;\n",
    "\n",
    "struct Inst<T> {\n",
    "\tpull: || T,\n",
    "\tlabel: str,\n",
    "}\n",
    "\n",
    "fun follow<T>(v: T): || T {\n",
    "\tlet followed: Shared<Inst<T>> = Shared::new(Inst<T> { pull = || v, label = \"f\" });\n",
    "\t|| (followed.read().pull)()\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet s: Shared<Inst<i32>> = Shared::new(Inst<i32> { pull = || 1, label = \"s\" });\n",
    "\tprint((s.read().pull)());\n",
    "\tprint(s.read().label);\n",
    "\tprint(follow(\"x\")());\n",
    "}\n",
);

/// F58: `on_change` through the blanket `Flow` impl, reached under a generic
/// bound `S: Source<List<X>>` whose value is a `ListCell<X>`. The blanket's
/// binder `T` grounds from the provider `impl ListCell<type E> with
/// Source<List<E>>` as `List<E>` — written in the PROVIDER's binder, which
/// the substitution never bound, so the instance was refused by name ("an
/// unbound generic type parameter (parameter 1 of struct `ListCell`)"). The
/// provider's own binder now binds from the receiver. Two shapes: a toy
/// source and std's own `ListCell` observed through a generic.
#[test]
fn a_blanket_reached_through_a_list_source_bound_builds_the_same_on_both_backends() {
    let staged = stage();
    for (program, source) in [
        ("native_probe_f58_toy.vl", F58_TOY_PROBE),
        ("native_probe_f58_list_cell.vl", F58_LIST_CELL_PROBE),
    ] {
        std::fs::write(staged.join(program), source).expect("write the probe program");
        assert_eq!(
            compare(&staged, program),
            Verdict::Identical,
            "{program}: a blanket reached through a `Source<List<X>>` bound must build and \
             print the same on both backends"
        );
    }
}

const F58_TOY_PROBE: &str = concat!(
    "trait Src<T> {\n",
    "\tfun get(self): T;\n",
    "}\n",
    "struct LC<T> {\n",
    "\titems: List<T>,\n",
    "}\n",
    "impl LC<type T> with Src<List<T>> {\n",
    "\tfun get(self): List<T> {\n",
    "\t\tself.items\n",
    "\t}\n",
    "}\n",
    "trait Fl<T> {\n",
    "\tfun now(own self): T;\n",
    "}\n",
    "impl type S: Src<type T> with Fl<T> {\n",
    "\tfun now(own self): T {\n",
    "\t\tself.get()\n",
    "\t}\n",
    "}\n",
    "fun watch<S: Src<List<str>>>(source: S) {\n",
    "\tprint(source.now().len());\n",
    "}\n",
    "fun main() {\n",
    "\twatch(LC<str> { items = [\"a\", \"b\"] });\n",
    "}\n",
);

const F58_LIST_CELL_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::delta::ListCell;\n",
    "import std::reactive::{ Disposable, Source };\n",
    "\n",
    "fun watch<S: Source<List<str>>>(source: S) {\n",
    "\tlet watching = source.on_change(|list| print(i\"changed {list.len()}\"));\n",
    "\tsource.get();\n",
    "\twatching.dispose();\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet source: ListCell<str> = ListCell::new();\n",
    "\twatch(source);\n",
    "\tlet kept = source.on_change(|list| print(i\"kept {list.len()}\"));\n",
    "\tsource.push(\"b\");\n",
    "\tkept.dispose();\n",
    "}\n",
    "main();\n",
);

/// F59: a field read on a call whose return type is INFERRED — `fun make(..)
/// { Square { .. } }` has no written return, and neither has B460's checked
/// return — builds natively. The call's type was read off the written
/// signature alone, so `make(2).side` had no struct to name its field from.
#[test]
fn a_field_read_on_an_inferred_return_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_f59.vl"), F59_PROBE).expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_f59.vl"),
        Verdict::Identical,
        "a field read on an inferred return must build and print the same on both backends"
    );
}

const F59_PROBE: &str = concat!(
    "struct Square {\n",
    "\tside: i32,\n",
    "\tname: str,\n",
    "}\n",
    "fun make_square(side: i32) {\n",
    "\tSquare { side = side, name = \"sq\" }\n",
    "}\n",
    "fun relabel(side: i32) {\n",
    "\tlet made = make_square(side);\n",
    "\tSquare { side = made.side * 2, name = i\"{made.name}!\" }\n",
    "}\n",
    "fun main() {\n",
    "\tprint(make_square(2).side);\n",
    "\tprint(relabel(3).side);\n",
    "\tprint(relabel(3).name);\n",
    "}\n",
);

/// A142 parity: an `Option` of a CELL holding closures — an owner's lazily
/// allocated cleanup list, `Option<Shared<List<|| void>>>` — has reference
/// equality natively (a cell compares by identity, as the JS object does), so
/// the struct holding it builds. It was refused by name, and reactive-44
/// wrote S2's `OwnerCell` around the refusal.
#[test]
fn an_option_of_a_cell_of_closures_builds_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_option_cell.vl"),
        OPTION_CELL_PROBE,
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_option_cell.vl"),
        Verdict::Identical,
        "an `Option` of a cell of closures must build and print the same on both backends"
    );
}

const OPTION_CELL_PROBE: &str = concat!(
    "import std::shared::Shared;\n",
    "\n",
    "struct Cell {\n",
    "\tcleanups: Option<Shared<List<|| void>>>,\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet c: Shared<Cell> = Shared::new(Cell { cleanups = None });\n",
    "\tmatch c.read().cleanups {\n",
    "\t\tSome(let list) => list.write().push(|| print(\"x\")),\n",
    "\t\tNone => {\n",
    "\t\t\tc.write() = Cell { cleanups = Some(Shared::new([|| print(\"first\")])) };\n",
    "\t\t},\n",
    "\t}\n",
    "\tmatch c.read().cleanups {\n",
    "\t\tSome(let list) => list.write().push(|| print(\"second\")),\n",
    "\t\tNone => {},\n",
    "\t}\n",
    "\tmatch c.read().cleanups {\n",
    "\t\tSome(let list) => {\n",
    "\t\t\tfor f in list.read() {\n",
    "\t\t\t\tf();\n",
    "\t\t\t}\n",
    "\t\t},\n",
    "\t\tNone => {},\n",
    "\t}\n",
    "}\n",
);

/// B470's native half: a `[resource]` trait's object is MOVED into its
/// consuming members. `Flow` is declared `[resource]` (A142 R39), so a
/// `dyn Flow<i32>` is move-only and its pointer unique; a slot whose member
/// takes `own self` takes the pointer (`self: Rc<Self>`) and moves the value
/// out of it (`vilan_rt::unshare`), where it copied the pipe out from behind a
/// borrow. Two programs: the mixed-arm selector (`dyn_objects`'
/// `a142_a_mixed_arm_selector_…`), whose arms are a `Derive` pipe and a root,
/// and a `dyn Flow` handed to an `own` parameter and consumed once.
#[test]
fn a_resource_trait_object_is_moved_into_its_consuming_members_natively() {
    let staged = stage();
    for (program, source) in [
        ("native_probe_b470_selector.vl", B470_SELECTOR_PROBE),
        ("native_probe_b470_once.vl", B470_ONCE_PROBE),
    ] {
        std::fs::write(staged.join(program), source).expect("write the probe program");
        assert_eq!(
            compare(&staged, program),
            Verdict::Identical,
            "{program}: a `dyn Flow` must build and print the same on both backends"
        );
        let emitted = vilan(&staged)
            .args(["build", "--backend", "rust", "--stdout", program])
            .output()
            .expect("build the probe");
        let source = String::from_utf8_lossy(&emitted.stdout);
        // The table's consuming slots move the value out of the pointer.
        let slots: Vec<&str> = source
            .lines()
            .filter(|line| line.contains("fn start(") || line.contains("fn on_change("))
            .filter(|line| line.trim_start().starts_with("fn "))
            .collect();
        assert!(
            !slots.is_empty()
                && slots
                    .iter()
                    .all(|line| line.contains("self: std::rc::Rc<Self>")),
            "{program}: a consuming slot must take the object's pointer:\n{}",
            slots.join("\n")
        );
        assert!(
            source.contains("vilan_rt::unshare(self)") && !source.contains("(self.clone(),"),
            "{program}: the object's value must be moved out, not copied:\n{source}"
        );
    }
    // The object handed to an `own` parameter is consumed by its one call:
    // the parameter's pointer is given over, not bumped and copied.
    let emitted = vilan(&staged)
        .args([
            "build",
            "--backend",
            "rust",
            "--stdout",
            "native_probe_b470_once.vl",
        ])
        .output()
        .expect("build the probe");
    let source = String::from_utf8_lossy(&emitted.stdout);
    let watch = source
        .split("\nfn ")
        .find(|function| function.starts_with("watch_"))
        .unwrap_or_else(|| panic!("no `watch` in the emitted source:\n{source}"));
    assert!(
        watch.contains(").into_object()") && !watch.contains(".clone().into_object()"),
        "the `own` object must be handed over whole:\n{watch}"
    );
}

const B470_SELECTOR_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::reactive::{ Flow, MemoCell, Signal, SignalCell, Source };\n",
    "\n",
    "fun arm(on: bool, count: SignalCell<i32>): dyn Flow<i32> {\n",
    "\tif on {\n",
    "\t\tcount.derive(|value| value * 100)\n",
    "\t} else {\n",
    "\t\tcount\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet flag = Signal::new(true);\n",
    "\tlet count = Signal::new(1);\n",
    "\tlet picked: MemoCell<i32> = flag.switch<i32, dyn Flow<i32>>(|on: bool| arm(on, count)).memo();\n",
    "\tprint(i\"{picked.get()}\");\n",
    "\tflag.set(false);\n",
    "\tcount.set(3);\n",
    "\tprint(i\"{picked.get()}\");\n",
    "}\n",
    "\n",
    "main();\n",
);

const B470_ONCE_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::reactive::{ Disposable, Flow, Signal, SignalCell, Source, Subscription };\n",
    "\n",
    "fun pick(on: bool, count: SignalCell<i32>): dyn Flow<i32> {\n",
    "\tif on {\n",
    "\t\tcount.derive(|value| value + 1)\n",
    "\t} else {\n",
    "\t\tcount\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun watch(label: str, own flow: dyn Flow<i32>): Subscription {\n",
    "\tflow.on_change(|value| print(i\"{label} {value}\"))\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet count = Signal::new(1);\n",
    "\tlet piped = pick(true, count);\n",
    "\tlet a = watch(\"piped\", piped);\n",
    "\tlet b = watch(\"root\", pick(false, count));\n",
    "\tcount.set(5);\n",
    "\ta.dispose();\n",
    "\tb.dispose();\n",
    "\tcount.set(9);\n",
    "\tprint(\"done\");\n",
    "}\n",
    "\n",
    "main();\n",
);

/// F62: a field read or written through a `Shared` view builds natively and
/// prints what node prints.
///
/// It was refused by name ("a field read of an unresolved subject"): the view
/// call records no type, so the field had no struct to be found in — which is
/// why A142's `OwnerCell` and S6's `TrackRuns` were written WHOLE. Typing the
/// call was half of it. A read through the view holds no borrow past itself
/// now (`read_with`, one borrow per spine, copying only the field), and every
/// site that takes the cell's `borrow_mut` as a place — an assignment, a
/// mutating call's receiver, a `&mut` argument — settles what it reads first,
/// so `a.write().n = a.write().n + 1` and `cell.write().items.push(cell.read()
/// .n)` meet no live borrow. The second probe is the `OwnerCell` shape written
/// field by field, the shape reactive-45 may return to.
#[test]
fn a_field_read_or_written_through_a_shared_view_is_identical_on_both_backends() {
    let staged = stage();
    for (name, program) in [
        (
            "native_probe_shared_view_fields.vl",
            include_str!("native/shared_view_fields.vl"),
        ),
        (
            "native_probe_shared_view_places.vl",
            include_str!("native/shared_view_places.vl"),
        ),
    ] {
        std::fs::write(staged.join(name), program).expect("write the probe program");
        assert_eq!(
            compare(&staged, name),
            Verdict::Identical,
            "{name}: a field through a `Shared` view must read and write the same on both backends"
        );
    }
}

/// F57: every platform-bound corpus program the backend ACCEPTS prints what
/// node prints, the ones it refuses say which construct stopped them, and the
/// ones it builds today ([`PLATFORM_BOUND_REQUIRED`]) stay built. The census is
/// printed, as the corpus sweep's is: its refusals are the work list.
#[test]
fn every_platform_bound_program_is_identical_or_named() {
    let staged = stage();
    let programs = platform_bound_programs();
    let mut identical_programs: Vec<String> = Vec::new();
    let mut refused: Vec<(String, String)> = Vec::new();
    let mut broken = Vec::new();
    for program in &programs {
        match compare(&staged, program) {
            Verdict::Identical => identical_programs.push(program.clone()),
            Verdict::Refused(reason) => refused.push((program.clone(), reason)),
            Verdict::Broken(detail) => broken.push(format!("{program}: {detail}")),
        }
    }
    eprintln!(
        "platform-bound differential: {} enumerated, {} identical, {} refused by name, {} broken",
        programs.len(),
        identical_programs.len(),
        refused.len(),
        broken.len()
    );
    for (program, reason) in &refused {
        eprintln!("  refused  {program}: {reason}");
    }
    for program in &identical_programs {
        eprintln!("  identical  {program}");
    }
    assert!(
        broken.is_empty(),
        "platform-bound programs the native backend ACCEPTED and then got wrong:\n{}",
        broken.join("\n")
    );
    for required in PLATFORM_BOUND_REQUIRED {
        assert!(
            identical_programs.iter().any(|program| program == required),
            "{required} builds natively and prints node's bytes; it must stay that way"
        );
    }
    for (program, _) in PLATFORM_BOUND_OUTSIDE {
        assert!(
            corpus_dir().join(program).is_file(),
            "{program} is named outside the platform-bound differential but is not a corpus program"
        );
    }
}

/// F57's second defect: a `match` literal pattern takes the SUBJECT's width,
/// never the expectation around the `match` (the arms' type). It stood behind
/// `crypto.vl`'s E0382 in `std::base64::decode_url`, so the platform-bound pin
/// reaches it only through a platform; this probe holds it platform-free.
#[test]
fn a_literal_pattern_takes_its_subjects_width_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_literal_pattern_width.vl"),
        include_str!("native/literal_pattern_width.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_literal_pattern_width.vl"),
        Verdict::Identical,
        "a literal pattern over a `usize`/`u8`/`u53` subject must build and match the same"
    );
}

/// F60, R-d door (a)'s native half: a pipe that is never consumed — built as a
/// statement and dropped — builds natively and the program runs as it does on
/// JS. The pipe nodes' `[must_use]` (reactive-45) makes the drop a WARNING on
/// both backends; this pin holds the build.
///
/// The item's repro, `outer.flatten();` over a `SignalCell<SignalCell<i32>>`,
/// is not a dropped-pipe defect: KEPT (`let kept = outer.flatten();`) it is
/// refused natively the same way, because the `flatten` it selects is the
/// blanket over `Flow<Option<I: Source<U>>>`, whose bound that receiver does
/// not meet, so `I` and `U` bind to nothing. B476/B477 refuse that selection in
/// the analyzer. Every well-typed dropped pipe already built; this keeps it so.
#[test]
fn a_dropped_pipe_builds_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_dropped_pipes.vl"),
        include_str!("native/dropped_pipes.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_dropped_pipes.vl"),
        Verdict::Identical,
        "a pipe built and dropped must build and run the same on both backends"
    );
}

/// F61: an injected closure's hidden parameters take ONE order natively — the
/// contexts' declaration order, the context pass's own — at the closure literal
/// and at the closure TYPE. The type had followed the clause as written, so a
/// clause out of declaration order (`context (second, first)`) typed its slots
/// `(str, i32)` while the literal landing there took `(i32, str)`, and rustc
/// refused it; std wrote every clause in declaration order, with a comment, to
/// keep out of the way. Two contexts backwards, three rotated with a value
/// parameter ahead of them, and a struct field carrying a backwards clause.
#[test]
fn an_injected_clause_written_out_of_order_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_clause_order.vl"),
        include_str!("native/clause_order.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_clause_order.vl"),
        Verdict::Identical,
        "a `context` clause in any written order must build and thread the same values"
    );
}

/// F63: a closure's expression body is the value it hands back, a CONSUMING
/// position like a function body's tail, so a place read there takes rule 1's
/// copy. `id.derive(|index| cells[index])` moved a `str` out of the captured
/// `Vec` (rustc E0507), and `|| row.name` moved a field out of its capture,
/// which made the closure `FnOnce` where every closure type is a `dyn Fn`.
#[test]
fn a_closure_handing_back_an_indexed_element_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_indexed_closure_reads.vl"),
        include_str!("native/indexed_closure_reads.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_indexed_closure_reads.vl"),
        Verdict::Identical,
        "a closure handing back an element or a field of a capture must copy it"
    );
}

/// F64: an `Option` of a closure as a field of a GENERIC struct builds natively
/// — stored through a cell's write view, pulled per element, read by a
/// `match` — and a `match` leg handing back a place copies it.
///
/// Two defects stood in front of the field, and neither was the field. A struct
/// literal pushed through `core.holds.write()` had no element type to close
/// `Hold<V>`'s `V` with, because the view call recorded none — refused by name
/// as "an unbound generic type parameter (parameter 1 of struct `Hold`)"; F62
/// typed the view. And a leg's body was read as a plain value, so `None =>
/// self.fallback` over a loaned `self` (E0507) and `None => before` read again
/// after the `match` (E0382) moved; a leg is a block tail's position now.
/// std's collection core (`ElementHold`) holds its closures bare to avoid this
/// shape; with `Option` fields it builds and prints node's bytes.
#[test]
fn an_optional_closure_field_of_a_generic_struct_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_option_closure_fields.vl"),
        include_str!("native/option_closure_fields.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_option_closure_fields.vl"),
        Verdict::Identical,
        "an `Option<|| V>` field of a generic struct must build and read the same"
    );
}

/// F52: a module-level `lazy let` handed to a `lazy` parameter. Natively the
/// binding is a `thread_local!` initialized at its first read — the deferral
/// `lazy` promises — and not a `Lazy` cell, so the analyzer's FORWARD (hand
/// the cell on) named a local nothing declares, and a binding reached only
/// that way was never emitted (rustc E0425). The parameter takes a thunk that
/// reads the binding: never forced, the initializer never runs; forced twice,
/// it runs once; forwarded on, it still runs once.
#[test]
fn a_lazy_module_binding_handed_to_a_lazy_parameter_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_lazy_module_forward.vl"),
        include_str!("native/lazy_module_forward.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_lazy_module_forward.vl"),
        Verdict::Identical,
        "a module `lazy let` at a `lazy` parameter must defer and initialize once"
    );
}

/// F54: a comparison answers a `bool`, so the expectation around it is never
/// its operands' — a literal in it takes its comparand's type.
/// `let two = if n > 2 { 1 } else { 2 }` over `n: u53` rendered `2` at the
/// arms' `i32` (the `if`'s value expectation reached its condition), and rustc
/// refused `u64 > i32` (E0308). `&&`/`||` beside `i16`/`f64` arms, a `bool`
/// binding, and a `while` condition inside an `i32`-valued block hold the same
/// rule.
#[test]
fn a_comparison_literal_takes_its_comparands_width_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_comparison_literal_width.vl"),
        include_str!("native/comparison_literal_width.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_comparison_literal_width.vl"),
        Verdict::Identical,
        "a comparison's literal must take its comparand's width"
    );
}

/// F55: an annotated `Option`/`Result` binding nothing reads builds natively.
/// The binding's annotation is not written (a view binding's type is its
/// pointee's, so writing every annotation would be wrong), and `Option` and
/// `Result` are Rust's own enums, whose bare path names no instance — so
/// `let ok: Result<i32, str> = Ok(10)`, never read, left `E` open (rustc
/// E0282). A written annotation over an `Option`/`Result` variant is written
/// natively now. (Writing the variant's own type arguments instead —
/// `Ok::<i32, Str>(10)` everywhere — was tried and is wrong: the arguments a
/// constructor records inside a generic instance are not reliable, and Rust's
/// inference had been covering for them.)
#[test]
fn an_unread_annotated_variant_binding_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_unread_annotated_variants.vl"),
        include_str!("native/unread_annotated_variants.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_unread_annotated_variants.vl"),
        Verdict::Identical,
        "an annotated `Option`/`Result` binding nothing reads must build"
    );
}

/// M91: a churned native `HashMap`/`HashSet` walks its live entries in the
/// order node's `Map` does — insertion order, a removed-then-re-inserted key at
/// the end, an overwritten one in place — through the compactions the native
/// map makes as its removed slots come to outnumber its live ones. The bound
/// itself (slots at most twice the live count, the storage released) is
/// `vilan-rt`'s `a_churned_map_compacts_and_keeps_insertion_order`: this suite
/// reads no clock (N116), so the 2,000 walks here are the shape, not a timing.
#[test]
fn a_churned_hash_map_walks_in_insertion_order_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_churned_map_walk.vl"),
        include_str!("native/churned_map_walk.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_churned_map_walk.vl"),
        Verdict::Identical,
        "a churned map and set must walk in insertion order on both backends"
    );
}

/// F65 (native-45 STOPPED it as the analyzer's; solver-b-45 fixed the record):
/// a generic function whose return is inferred, called at two instantiations,
/// emitted ONE instantiation's return type for every instance. `wrap<T>(x: T):
/// Source<T> { SignalCell::new(x) }` at `i32` and `str` emits both instances
/// returning the `str` cell, and the `i32` caller's reads meet the wrong
/// struct (rustc E0308). The same holds with no return written at all, so it
/// is not B460's opacity: `inferred_return_types` records the function's
/// return as the LAST call site's `SignalCell<str>` rather than
/// `SignalCell<T>`, and both call expressions share that one type id — so
/// nothing per call carries the instance's return for the emitter to read.
/// The record is written only under an empty substitution now — in the
/// function's own terms — so each instance substitutes its own.
#[test]
fn a_generic_inferred_return_is_per_instance_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_generic_inferred_return.vl"),
        include_str!("native/generic_inferred_return.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_generic_inferred_return.vl"),
        Verdict::Identical,
        "each instance of a generic inferred-return function returns its own type"
    );
}

/// B473: a member a supertrait declares, reached through a SUB-trait — a
/// bound, a deeper bound over a parameterized chain, a qualified call at a
/// generic and at a concrete receiver, a `for` loop's `next` — answers out of
/// the receiver's impl of the declaring trait: the override, then the default.
/// Both backends took the default over the override (`inference::traits`'
/// `b473_*` pins hold the JS values; this is the native half).
#[test]
fn a_supertrait_override_is_dispatched_through_a_subtrait_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b473.vl"), B473_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b473.vl"),
        Verdict::Identical,
        "a supertrait's override must answer through a sub-trait on both backends"
    );
}

/// B467: a closure whose `&mut` is its own PARAMETER is stored in a struct
/// field and called through it on both backends — the analyzer refused it as a
/// view escape before. (`inference::borrows`' `b467_*` pin covers the other
/// depths on JS; natively a closure VALUE reached through a match capture, a
/// loop binding or a nested closure parameter does not yet carry its view
/// parameters to the call site — a native find filed from this lane.)
#[test]
fn a_closure_with_its_own_view_parameter_is_stored_in_a_field_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b467.vl"), B467_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b467.vl"),
        Verdict::Identical,
        "a closure with its own view parameter must store and run the same on both backends"
    );
}

/// R-c (B483 + B466 + B465): a value taken out of a place the binding does not
/// own is copied, on both backends. `Shared::new`/`ListCell::of`/`with_limit`
/// take their argument `own` (natively the constructor's argument was MOVED,
/// rustc E0382 at the caller's next read); `*view` of an aggregate is copied
/// where it is bound, returned or wrapped (natively a move out of a reference,
/// E0507); and a closure literal at a view position of its closure type reads
/// the element through `*c` (natively a value was passed where the type wants
/// a reference). `inference::borrows`' `b483_*`/`b466_*`/`b465_*` pins hold
/// the JS values.
#[test]
fn a_value_taken_out_of_a_place_it_does_not_own_is_copied_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_rc.vl"), RC_PROBE).expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_rc.vl"),
        Verdict::Identical,
        "a value taken out of a place it does not own must be a copy on both backends"
    );
}

/// B474: `sub` called through an `S: Source<T>` bound is `Flow`'s member (the
/// trait answers through a bound, B408's rule) — not `RemoteSource`'s
/// inherent `sub(|T|)`, which the native emitter picked by name and rustc
/// refused (E0308) — while the same `sub` on the concrete mirror is still the
/// inherent one. `inference::markdown`'s
/// `a52_the_inherent_rpc_sub_outranks_the_traits_and_still_skips_the_none` is
/// the JS half.
#[test]
fn sub_through_a_source_bound_takes_the_trait_member_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b474.vl"), B474_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b474.vl"),
        Verdict::Identical,
        "`sub` through a `Source` bound must dispatch to the trait's member on both backends"
    );
}

/// B453: a view of a tuple position writes and reads through on both
/// backends (`inference::tuples`' `b453_*` pin is the JS half; JS threw).
#[test]
fn a_view_of_a_tuple_position_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b453.vl"), B453_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b453.vl"),
        Verdict::Identical,
        "a view of a tuple position must write through on both backends"
    );
}

/// B444: a scalar view read where a value is read prints the element on both
/// backends (`inference::borrows`' `b444_*` pin is the JS half; JS printed the
/// `(base, key)` pair).
#[test]
fn a_scalar_view_read_as_a_value_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b444.vl"), B444_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b444.vl"),
        Verdict::Identical,
        "a scalar view read as a value must print the element on both backends"
    );
}

/// B464: a closure's `&mut` parameter called with `&mut <place>` — a field,
/// a local, a subscript — writes through the same on both backends (the bare
/// spelling is refused; JS threw on it).
#[test]
fn a_closure_view_parameter_takes_a_view_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b464.vl"), B464_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b464.vl"),
        Verdict::Identical,
        "a closure's view parameter must write through on both backends"
    );
}

/// B504: a view of a pattern binder — a `match` capture (concrete and
/// generic), a destructured `let`'s, a `for` element, an `is` capture — reads
/// the binder on both backends (`inference::borrows`' `b504_*` pins are the
/// JS half; JS read the value's first character, or `undefined`).
#[test]
fn a_view_of_a_pattern_binder_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b504.vl"), B504_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b504.vl"),
        Verdict::Identical,
        "a view of a pattern binder must read the binder on both backends"
    );
}

/// B506: a view of a generic place — a field typed `T`, a parameter typed
/// `T`, a view binding of one, an inline transient's capture, a `mut x: T`
/// parameter's `&mut x`, `&mut pair.left` written through, a re-borrow — is
/// the same on both backends at a scalar and an aggregate instance, and a
/// view RETURNED keeps its protocol (`*first(..)`, `*left_of(..)`).
#[test]
fn a_view_of_a_generic_place_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b506.vl"), B506_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b506.vl"),
        Verdict::Identical,
        "a view of a generic place must be decided per instance on both backends"
    );
}

/// B505: a scalar receiver at a `&self`/`&mut self` — a local, a literal, an
/// rvalue, a field, an element, a tuple position, a `bool`, a generic — and a
/// view of a scalar with no cell (`&11`, `&(a + 8)`, an immutable or closure
/// parameter) are identical on both backends; JS passed the value bare.
#[test]
fn a_scalar_receiver_and_a_cell_less_view_are_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b505.vl"), B505_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b505.vl"),
        Verdict::Identical,
        "a scalar receiver at a view self must be a view on both backends"
    );
}

/// B496: the spelled copy of a view expression assigned into a value place
/// (`out = *&a`, `out = *inner(&holder)`, `n = *&m`) copies on both backends
/// (the unspelled assignment is refused; JS aliased it).
#[test]
fn the_spelled_copy_of_a_view_expression_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_b496.vl"), B496_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_b496.vl"),
        Verdict::Identical,
        "the spelled copy of a view expression must copy on both backends"
    );
}

/// F72: a variant constructor of an enum whose parameter is BOUNDED builds
/// natively — `enum Op<T: Hashable> { Add(T), Drop(T) }` then
/// `let op: Op<i32> = Op::Add(3)`, which was refused as "a value of type `a
/// trait object`". The constructor's site records the enum OPEN, its argument
/// the parameter's constraint id, and that id's type is the parameter's bound:
/// `any` for an unbounded `T` (read as open, so the emitter fell back to the
/// position or the payload) but `Hashable` itself for a bounded one, which the
/// emitter took for a closed argument and minted the enum over. That is why a
/// second, unbounded parameter (`MapOp<K: Hashable, V>`) built: one open
/// argument sent the whole list to the fallback. The probe covers an annotated
/// binding, the payload alone, an argument position, `Option` and list
/// nesting, a generic function at two instances, a generic impl, two bounds
/// on one parameter, and two bounded parameters. It blocked `HashSetCell`,
/// `SetOp` and `HashMapCell::keys()`.
#[test]
fn a_variant_of_an_enum_with_a_bounded_parameter_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_bounded_variant_constructors.vl"),
        include_str!("native/bounded_variant_constructors.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_bounded_variant_constructors.vl"),
        Verdict::Identical,
        "a variant of an enum whose parameter is bounded must build and print the same"
    );
}

/// F76: a bare variant of a generic USER enum — `let n: Maybe<i32> =
/// Maybe::Nothing` — builds natively; it was refused as "an unbound generic
/// type parameter (parameter 1 of enum `Maybe`)" under its own annotation. A
/// bare variant is an `Expr::Local` of the variant's declaration, and its arm
/// minted the enum at NO arguments, never asking the position — where a
/// constructor with a payload reads the site's record, then the position, then
/// the payload (`variant_arguments`), and a bare one has only the first two.
/// It takes that rule now, and a constructor's payload is a position of its
/// own (`Some(Maybe::Nothing)` under `Option<Maybe<i32>>`). `None` built all
/// along because `Option` is Rust's enum, whose path names no instance. The
/// probe covers an annotated binding, an argument, a generic function's
/// return at two instances, `ret` and a tail, a mixed list literal, an
/// assignment, a nested payload, both arms of an `if`, a field written
/// through a generic impl, and a two-parameter enum with a bound.
#[test]
fn a_bare_variant_of_a_generic_enum_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_bare_generic_variants.vl"),
        include_str!("native/bare_generic_variants.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_bare_generic_variants.vl"),
        Verdict::Identical,
        "a bare variant of a generic enum must close from its position and print the same"
    );
}

/// F66: a variant constructor INSIDE a generic body closes its enum's
/// arguments per instance. The analyzer records one type per SITE, and
/// `variant_arguments` read that record first: inside `Maybe<T>::map<U>` the
/// site of `Maybe::Just(f(x))` recorded the receiver's `Maybe<T>`, so the
/// `(str, i32)` instance minted `Maybe<(str, i32)>` for a `Maybe<i32>` value
/// and rustc refused the emission (E0308 four times on 0.43.0). The position
/// and the payload are read under the instance and come first; a record closed
/// in itself (no parameter in it) still wins, being the same in every
/// instance. The probe: a payload of the method's own parameter, a nullary
/// variant at a generic return, a two-parameter enum built swapped (two
/// instances each way), a nested payload, a binding annotated in the
/// instance's parameter, and `Option`'s constructors inside a generic body.
#[test]
fn a_variant_built_inside_a_generic_instance_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_generic_instance_variants.vl"),
        include_str!("native/generic_instance_variants.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_generic_instance_variants.vl"),
        Verdict::Identical,
        "a variant built inside a generic instance must take that instance's arguments"
    );
}

/// A closure's expression body is the closure's RETURN position (native-46's
/// find, beside F66). `closure_body` handed the body the expectation the
/// literal arrived under — the closure TYPE — so a variant constructor whose
/// site recorded an open type (`|k: i32| Maybe::Just(k + 1)`, recorded
/// `Maybe<any>`, a payload with no record of its own) had nothing to close
/// from and was refused as "a generic type instantiated at `any`". The body
/// now takes the position's return when it is closed, else the literal's
/// written one. The probe: a sum, a string literal, an interpolation and a
/// nested constructor as payloads, an `if` choosing between two constructors,
/// a written return (also handed to a generic callee), and a literal at a
/// narrow `u8` return.
#[test]
fn a_closure_body_takes_the_closures_return_as_its_position_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_closure_body_positions.vl"),
        include_str!("native/closure_body_positions.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_closure_body_positions.vl"),
        Verdict::Identical,
        "a closure body must be emitted at the closure's return position"
    );
}

/// F70: a NESTED tuple access natively. The analyzer folds `t.0.1` onto its
/// root and records the JS layout's FLAT offset, and the emitter wrote that
/// offset as a Rust tuple index — `t.0.1` over `((1, 2), 3)` read `t.1` and
/// printed `3` where node prints `2` (a wrong answer with no error whenever the
/// neighbour has the same type; rustc's E0308 otherwise), and a multi-slot
/// element was refused by name. The recorded index chain
/// (`tuple_index_paths`) is the Rust path. The probe: a same-typed
/// neighbour, a write, a compound write, a `&mut` handed on, a multi-slot read,
/// a destructure of a nested element, three levels, a `Shared` view read and
/// write, and a generic function at an instance whose parameter is a tuple.
#[test]
fn a_nested_tuple_access_reads_the_nested_element_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_nested_tuple_slots.vl"),
        include_str!("native/nested_tuple_slots.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_nested_tuple_slots.vl"),
        Verdict::Identical,
        "a nested tuple access must read the element its index chain names"
    );
}

/// F75: a trait DEFAULT reached through the `Flow` blanket over a generic
/// source whose `Source` argument is written in the source impl's own binder
/// — `impl W<type P> with Source<Option<P>>`, then `w.effect(..)` — builds
/// natively; it was refused as "an unbound generic type parameter (parameter
/// 1 of struct `W`)". The default's substitution binds the blanket's `T` from
/// the receiver's `Source` impl, as `Option<P>` in that PROVIDER's binder, and
/// nothing bound `P`: F58 had added the provider's binders where a MEMBER is
/// dispatched (`on_change`, `start`), not where a default is specialized
/// (`effect`, `effect_on_change`). The probe covers both, a two-parameter
/// source whose argument is in its second parameter, a list argument, and the
/// pipes (`derive(..).memo()`, `sample()`). It blocked observing std's
/// `StoreSome<P>` natively.
#[test]
fn a_default_over_a_source_written_in_its_providers_binder_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_provider_binder_defaults.vl"),
        include_str!("native/provider_binder_defaults.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_provider_binder_defaults.vl"),
        Verdict::Identical,
        "a Flow default over a source written in its provider's binder must build and run the same"
    );
}

/// F73: `set(None)` on a two-parameter struct's `Signal<Option<V>>` impl, and
/// the struct literal that builds it, are identical on both backends.
///
/// The item's own refusal ("an unbound generic type parameter (parameter 2 of
/// struct `Pair`)" at the `impl .. with Signal<Option<V>>` header) reproduces
/// on 0.42.0 and not on this order's base, where an earlier merge had closed
/// it; the probe pins it. Reproducing it found the literal beside it: a struct
/// literal's field expectation was its declared type's HEAD, resolved under the
/// instance and then dropped, so `cell = SignalCell::new(None)` met
/// `SignalCell<Option<V>>` with the struct's own `V` unbound. The instance's
/// bindings stay in force while a field's value is rendered now — in `main`
/// under an annotation, in the struct's own static at two instantiations, and
/// for a generic struct literal nested in another's field.
///
/// Inside the struct's OWN impl a literal of ANOTHER instantiation
/// (`Pair<V, K>` in `impl Pair<type K, type V>`) cannot take that rule — the
/// impl's binders are the declaration's parameters, and installing the
/// literal's bindings would retype the value's own reads. F82: the field's
/// type is MINTED with the literal's arguments written in there, so the
/// swapped literal's `held = Maybe::Nothing` closes from `Maybe<str>` and the
/// probe below is identical (it was refused by name before, and accepted here
/// only as that refusal).
#[test]
fn a_struct_literals_fields_close_their_values_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_struct_literal_field_positions.vl"),
        include_str!("native/struct_literal_field_positions.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_struct_literal_field_positions.vl"),
        Verdict::Identical,
        "a struct literal's field must close its value from the instance on both backends"
    );
    std::fs::write(
        staged.join("native_probe_swapped_literal.vl"),
        SWAPPED_LITERAL_PROBE,
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_swapped_literal.vl"),
        Verdict::Identical,
        "a swapped literal in its own impl must close its fields from its own arguments"
    );
}

/// F82: a struct literal of another instantiation inside the struct's own
/// impl closes EVERY field from the field's type read under the literal's
/// arguments — the emitter mints `Maybe<str>` from `held: Maybe<V>` where
/// the literal's `V` is the method's `K` (`Emitter::substituted`). The probe:
/// a nullary variant, `None`, an empty list, a cell around `None`, a tuple of
/// both parameters, a bare parameter, a field naming no parameter and a
/// closure field, swapped twice (so both instantiations build each way).
#[test]
fn a_swapped_struct_literal_closes_every_field_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_swapped_struct_literals.vl"),
        include_str!("native/swapped_struct_literals.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_swapped_struct_literals.vl"),
        Verdict::Identical,
        "a swapped struct literal must close every field from its own instantiation"
    );
}

const SWAPPED_LITERAL_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "enum Maybe<T> {\n",
    "\tNothing,\n",
    "\tJust(T),\n",
    "}\n",
    "\n",
    "struct Pair<K, V> {\n",
    "\tkey: K,\n",
    "\theld: Maybe<V>,\n",
    "}\n",
    "\n",
    "impl Pair<type K, type V> {\n",
    "\tfun swap(self, value: V): Pair<V, K> {\n",
    "\t\tPair { key = value, held = Maybe::Nothing }\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet pair: Pair<str, i32> = Pair { key = \"k\", held = Maybe::Just(1) };\n",
    "\tlet swapped = pair.swap(9);\n",
    "\tprint(swapped.key);\n",
    "\tprint(swapped.held is Maybe::Nothing);\n",
    "}\n",
);

/// F71: a pipe built and SEALED inside a generic body — `source.derive(|v|
/// f(v)).switch(|inner| inner).cell()` in `fun switch_to<T, U, S: Source<T>,
/// I: Source<U>>` — builds natively; it was refused as "an unbound generic
/// type parameter (parameter 3 of `switch_to`)". The sealing call is a trait
/// DEFAULT whose receiver is written in the generic body's binders
/// (`Switch<Derive<S, ..>, ..>`), and a default body ran under the trait's
/// bindings ALONE — the caller's `S` replaced away — where a function instance
/// composes onto its caller's. The probe covers the item's repro, B479's
/// unannotated selector (`inference::traits`' pin is its JS half), `memo()`
/// and `sample()` as the seal, the blanket-method spelling, and an `Option`
/// source.
#[test]
fn a_pipe_sealed_in_a_generic_body_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_generic_pipe_defaults.vl"),
        include_str!("native/generic_pipe_defaults.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_generic_pipe_defaults.vl"),
        Verdict::Identical,
        "a pipe sealed in a generic body must build and run the same on both backends"
    );
}

/// F74: an in-process `duplex_pair` program — A146's inference pin, a server
/// and a client mirroring a cell and a keyed cell, each source wrapped in a
/// generic `Counted<S>` — builds natively and prints node's bytes; it was
/// refused as "an unresolved type". The wrapper was not the cause: the program
/// reaches `std::rpc`'s `keyed_mirror_of`, and two of its bindings were typed
/// only by their written annotations. `Shared::new(|_key| {})` under
/// `Shared<|Hash| void>` — the analyzer leaves a closure parameter nothing
/// constrains untyped, so the literal takes its parameter types from the
/// closure type it is rendered into; and `Shared::new([])` under
/// `Shared<List<KeyLease>>`, whose first use reads `lease.key` off a `Vec<_>`
/// rustc had not settled (E0282) — a cell around an empty literal has its
/// binding's type written, as an empty literal always had. The second program
/// holds both shapes outside std.
#[test]
fn an_in_process_mirror_program_is_identical_on_both_backends() {
    let staged = stage();
    for (name, program) in [
        (
            "native_probe_in_process_mirrors.vl",
            include_str!("native/in_process_mirrors.vl"),
        ),
        (
            "native_probe_position_typed_bindings.vl",
            include_str!("native/position_typed_bindings.vl"),
        ),
    ] {
        std::fs::write(staged.join(name), program).expect("write the probe program");
        assert_eq!(
            compare(&staged, name),
            Verdict::Identical,
            "{name}: must build and print the same on both backends"
        );
    }
    // A146's mirror half, natively: one edge per source across the runs.
    let output = vilan(&staged)
        .args([
            "run",
            "--backend",
            "rust",
            "native_probe_in_process_mirrors.vl",
        ])
        .output()
        .expect("run the probe natively");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "list attaches=1\nkeyed cell attaches=1\nmirror attaches=1\nkeyed mirror attaches=1\n5 2 1 7 1\n",
        "one edge per source, mirrors included, natively (A146)"
    );
}

/// B519: a `[service(Client)]` declared in a module the entry IMPORTS. Its
/// generated module-level `let __mirrors_StoreClient_<method>` tables are the
/// module's bindings on both backends — natively the table was an E0425
/// ("not found in this scope") in the emitted Rust, on JS a `ReferenceError`
/// at the first stub call — and the stubs dedup per origin the same way.
#[test]
fn b519_a_service_in_an_imported_module_is_identical_on_both_backends() {
    let staged = stage();
    for (name, program) in [
        ("b519_store.vl", include_str!("native/b519_store.vl")),
        (
            "native_probe_b519_imported_service.vl",
            include_str!("native/b519_imported_service.vl"),
        ),
    ] {
        std::fs::write(staged.join(name), program).expect("write the probe program");
    }
    assert_eq!(
        compare(&staged, "native_probe_b519_imported_service.vl"),
        Verdict::Identical,
        "a service in an imported module must build and print the same on both backends"
    );
    let output = vilan(&staged)
        .args([
            "run",
            "--backend",
            "rust",
            "native_probe_b519_imported_service.vl",
        ])
        .output()
        .expect("run the probe natively");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "same-origin:true\nsame-args:true\nother-args:false\nunleased:calls=0\ndone\n",
        "one mirror per origin, natively"
    );
}

/// F79: a `mut` pattern binder — `Some(mut p)` in a `match`, `mut (c, d) =
/// (3, 4)`, an `is` capture, a generic body's `match` — is emitted `mut`, so a
/// `&mut` of it builds (rustc E0596 before), and the matched place a program
/// reads again keeps its value.
#[test]
fn a_mut_pattern_binder_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_mut_pattern_binders.vl"),
        include_str!("native/mut_pattern_binders.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_mut_pattern_binders.vl"),
        Verdict::Identical,
        "a `mut` pattern binder must be mutable natively and print the same"
    );
}

/// F80: a `for e in &mut xs` element is a `&mut` loan natively (an `iter_mut`
/// item), so handing it on — `f(e)` to a `|&mut T|` closure in a generic body
/// or a concrete one, `bump(&mut counter)`, a field write through it — is a
/// reborrow (`&mut *e`). It was emitted `&mut e`, which rustc refused (E0596).
#[test]
fn a_for_mut_element_handed_on_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_for_mut_element_loans.vl"),
        include_str!("native/for_mut_element_loans.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_for_mut_element_loans.vl"),
        Verdict::Identical,
        "a `for` element over `&mut xs` must be handed on by reborrow and print the same"
    );
}

/// F81: `*if c { &a } else { &b }` — the spelled copy B496's refusal steers to
/// — builds natively over a list and a struct, through a `match`, and nested;
/// it was emitted as a deref of the branches' COPIES (rustc E0599/E0614). A
/// branch tail is a value position here, so each leaf is its copy already and
/// the `*` is dropped. (A SCALAR or `str` chosen this way prints the place pair
/// on JS — reported — so the probe holds aggregates.)
#[test]
fn a_deref_of_a_conditional_view_is_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_deref_of_a_conditional_view.vl"),
        include_str!("native/deref_of_a_conditional_view.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_deref_of_a_conditional_view.vl"),
        Verdict::Identical,
        "`*` over a conditional view must copy the chosen value on both backends"
    );
}

/// A142 S7 S1 (`proposal/store.md`): a derived struct's store builds and wakes
/// the same on both backends — a nested struct, a whole write that wakes only the
/// changed spine, a handle write committed along the spine, a write of the value
/// held, `notify`, a `[reactive(coarse)]` field, a `[reactive(name = "..")]`
/// projection, an `Option` field through its `Some` (observed, read and patched), a generic
/// struct, and the slot census through subscribe and dispose.
#[test]
fn a142_s7_a_struct_store_builds_and_wakes_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_store_struct.vl"),
        include_str!("native/store_struct.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_store_struct.vl"),
        Verdict::Identical,
        "a struct store must build and wake the same on both backends"
    );
}

/// A142 S7 S2: a derived enum's store builds and wakes the same on both
/// backends — a same-variant write patching the payload, a switch away and back,
/// a `patch` through a dead and a live variant, a multi-payload variant read and
/// patched as a tuple, `assume()` read past the variant's end, and a generic
/// enum with a scalar payload.
#[test]
fn a142_s7_an_enum_store_builds_and_wakes_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_store_variant.vl"),
        include_str!("native/store_variant.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_store_variant.vl"),
        Verdict::Identical,
        "an enum store must build and wake the same on both backends"
    );
}

/// B436 + B437: a trait object prints as its value, alone, and as the pair a
/// list holds; two applications of one trait over one type dispatch through
/// their own tables — identical on both backends (the JS leg printed the pair
/// and answered `Shape<str>`'s `area` from the `i32` table).
#[test]
fn a_trait_objects_print_and_its_tables_are_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_dyn_print_and_tables.vl"),
        include_str!("native/dyn_print_and_tables.vl"),
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_dyn_print_and_tables.vl"),
        Verdict::Identical,
        "an object prints its value and dispatches through its own application's table"
    );
}
