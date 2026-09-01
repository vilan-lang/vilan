//! Return checking and expression lifting: bare `ret`, the inferred return
//! type (B126), the reachable-tail rule (B124/B125/B133), diagnostic span
//! precision, `expr!`, `a?.b` and `a? + b?`.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- Bare `ret` (return void) -------------------------------------------------

// `ret` with no value is a void early-return: the guard exits before the print,
// and the non-guarded call falls through to it.
#[test]
fn bare_ret_returns_void_early() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun guard(flag: bool) {
        	if flag {
        		ret;
        	}
        	print("passed");
        }

        fun main() {
        	guard(true);
        	guard(false);
        }
        "#,
        "passed\n",
    );
}

// A `ret` value must match the declared return type (proposal/ret-checking.md:
// `ret` joins the tail's `ReturnType` constraint, which now verifies via
// `reconcile_type` instead of only directing inference).
#[test]
fn ret_value_is_checked_against_the_declared_return_type() {
    assert_fails(
        r#"
        fun bad(): i32 {
        	ret "nope";
        	1
        }

        fun main() {
        	let _ = bad();
        }
        "#,
    );
}

// The void case: a bare `ret` is `ret <void>` — legal exactly when the
// declared return type is void, rejected in a value-returning function.
#[test]
fn bare_ret_in_a_value_returning_function_is_rejected() {
    assert_fails(
        r#"
        fun bad(flag: bool): i32 {
        	if flag {
        		ret;
        	}
        	1
        }

        fun main() {
        	let _ = bad(true);
        }
        "#,
    );
}

// --- B152: a tail that LEAVES emits the statement, not a wrapped value ---------
//
// `ret` is an expression of the never type, so it may sit in a function's tail
// position — but JS `return` is a STATEMENT. Every seam that wrapped or assigned
// a walked tail used to do so blindly, emitting `return return 1;` (and
// `const y = return 1;`): a bundle that does not PARSE, so the failure is loud —
// `node` refuses the file rather than running it wrong. Each pin below runs the
// emitted bundle through `node`, which cannot happen unless it parses.

// The filed one-liner: a bare `ret <expr>` as the whole body.
#[test]
fn ret_in_tail_position_emits_one_return() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun a(): i32 { ret 1 }

        fun main() {
        	print(a());
        }
        "#,
        "1\n",
    );
}

// The void form: a bare `ret` as the whole body emitted `return return;`.
#[test]
fn bare_void_ret_in_tail_position_emits_one_return() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun a() { ret }

        fun main() {
        	a();
        	print("done");
        }
        "#,
        "done\n",
    );
}

// `ret { <expr> }` — the returned VALUE is a block, so the tail walks to a
// `return` through the block's own tail; the outer seam wrapped it again.
#[test]
fn ret_of_a_block_value_in_tail_position_emits_one_return() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun a(): i32 { ret { 1 } }

        fun main() {
        	print(a());
        }
        "#,
        "1\n",
    );
}

// A nested block whose tail leaves: the block reports no value and the `ret`
// lands in the enclosing block, where it is legal.
#[test]
fn ret_inside_a_nested_block_tail_emits_one_return() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun a(): i32 { { ret 1 } }

        fun main() {
        	print(a());
        }
        "#,
        "1\n",
    );
}

// …at any depth — the leak was the block's tail, so it composes.
#[test]
fn ret_inside_a_doubly_nested_block_tail_emits_one_return() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun a(): i32 { { { ret 1 } } }

        fun main() {
        	print(a());
        }
        "#,
        "1\n",
    );
}

// The value-position sibling: a leaving block bound by a `let` emitted
// `const y = return 1;`. The binding is unreachable, so the run never sees it.
#[test]
fn ret_inside_a_block_bound_by_a_let_emits_one_return() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun a(): i32 {
        	let y = { ret 1 };
        	y
        }

        fun main() {
        	print(a());
        }
        "#,
        "1\n",
    );
}

// The two sibling forms the filing named, which went through the arm seam's
// divergence check and were ALREADY correct — pinned so they stay that way.
#[test]
fn ret_in_both_if_tail_arms_emits_one_return_each() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun a(flag: bool): i32 { if flag { ret 1 } else { ret 2 } }

        fun main() {
        	print(a(true));
        	print(a(false));
        }
        "#,
        "1\n2\n",
    );
}

#[test]
fn ret_in_a_match_arm_emits_one_return() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun a(n: i32): i32 {
        	match n {
        		1 => ret 10,
        		_ => ret 20,
        	}
        }

        fun main() {
        	print(a(1));
        	print(a(2));
        }
        "#,
        "10\n20\n",
    );
}

// The two seams composed: a nested leaving block INSIDE a value-position arm.
// The arm's result temp is still named (so sibling arms and later temps keep
// their names) but nothing is assigned to it on the leaving path.
#[test]
fn ret_inside_a_nested_block_in_an_if_arm_emits_one_return() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun a(flag: bool): i32 { if flag { { ret 1 } } else { 2 } }

        fun main() {
        	print(a(true));
        	print(a(false));
        }
        "#,
        "1\n2\n",
    );
}

// --- Malformed frames are decode errors, never crashes -------------------------

// The JSON codec's reader must arrive PRE-POISONED on text that is not JSON at
// all (wire frames are untrusted input): `decode` returns `Err`, and an RPC
// protocol answers a garbage request with `Failure(Decode)` — it used to throw
// out of `JSON.parse`, letting one malformed request kill a server process.
#[test]
fn malformed_json_frames_fail_sticky_instead_of_crashing() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };
        import std::json::{ json_codec, decode_json };
        import std::wire::{ decode, Frame };
        import std::rpc::{ Dispatcher, reply, RpcOutcome, RpcError };

        fun main() {
        	// The decode seam: garbage text and a garbage binary frame both Err.
        	let direct: Result<i32, str> = decode_json("garbage{{{");
        	match direct {
        		Ok(let value) => print("direct: unexpected Ok"),
        		Err(let reason) => print(i"direct: {reason}"),
        	}
        	let framed: Result<i32, str> = decode(json_codec(), Frame::Text("also not json"));
        	match framed {
        		Ok(let value) => print("framed: unexpected Ok"),
        		Err(let reason) => print(i"framed: {reason}"),
        	}
        	// The RPC seam: a protocol ANSWERS a garbage request (Failure
        	// envelope), it does not throw.
        	let protocol = Dispatcher::new().on("ping", |request| reply(1)).into_protocol(json_codec());
        	let answer = protocol.respond(Frame::Text("garbage{{{"));
        	match answer {
        		Frame::Text(let envelope) => print(i"rpc answers: {envelope}"),
        		Frame::Binary(let bytes) => print("rpc: unexpected binary"),
        	}
        }
        "#,
        "direct: malformed JSON\nframed: malformed JSON\nrpc answers: {\"Failure\":{\"Decode\":\"malformed JSON\"}}\n",
    );
}

// The wider half of the same gap (proposal/ret-checking.md): the TAIL was not
// checked either — `Constraint::ReturnType` directed inference but never
// verified. `fun f(): i32 { "nope" }` used to compile clean.
#[test]
fn function_tail_is_checked_against_the_declared_return_type() {
    assert_fails(
        r#"
        fun bad(): i32 {
        	"nope"
        }

        fun main() {
        	let _ = bad();
        }
        "#,
    );
}

// A void CALL is not a value: caught in tail position...
#[test]
fn a_void_call_tail_is_not_a_value_return() {
    assert_fails(
        r#"
        import std::io::print;

        fun bad(): i32 {
        	print("side effect")
        }

        fun main() {
        	let _ = bad();
        }
        "#,
    );
}

// ...and in `ret` position.
#[test]
fn a_void_call_ret_is_not_a_value_return() {
    assert_fails(
        r#"
        import std::io::print;

        fun bad(): i32 {
        	ret print("side effect");
        	1
        }

        fun main() {
        	let _ = bad();
        }
        "#,
    );
}

// One bad `ret` among good ones is flagged — the check is per return site,
// not per function.
#[test]
fn one_bad_ret_among_good_ones_is_flagged() {
    assert_fails(
        r#"
        fun bad(a: bool, b: bool): i32 {
        	if a {
        		ret 1;
        	}
        	if b {
        		ret "two";
        	}
        	3
        }

        fun main() {
        	let _ = bad(true, false);
        }
        "#,
    );
}

// In a function with NO declared return type, a `ret` is return evidence
// like the tail (proposal/ret-checking.md rule 3, amended by B126): here a
// `ret` of a void call beside a body that ends without a value — both void,
// so they agree. Re-pinned from `ret_with_a_value_in_an_undeclared_void_
// function_is_allowed`, whose comment said the value was "discarded, not
// diagnosed"; it is neither now, it is read.
#[test]
fn b126_a_void_ret_agrees_with_a_void_body() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun loud(flag: bool) {
        	if flag {
        		ret print("early");
        	}
        	print("late");
        }

        fun main() {
        	loud(true);
        	loud(false);
        }
        "#,
        "early\nlate\n",
    );
}

// --- B126: an unannotated function's return type is inferred from its
// reachable tail AND every `ret` (proposal/ret-checking.md rule 3, amended
// 2026-08-22). Before: the `Type::Function` arm read the tail id alone, so
// `fun f(x: bool) { ret 1; }` was `void` at every call site and a `ret` of the
// wrong type was invisible — `{ if x { ret "s"; } 2 }` handed a `str` to an
// `i32` binding at runtime. One helper (`inferred_return_type`) now answers
// for the call site, closure coercion, the `for` protocol's `next`, and trait
// conformance; each pin below is one shape of the rule.

// The headline: a body that leaves only by `ret` has an unreachable
// (synthesized void) tail, which is no evidence. Plant-proven: dropping the
// rets from the evidence, or reading the dead tail as reachable, turns this red.
#[test]
fn b126_a_ret_only_body_infers_its_return_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun f(x: bool) {
        	ret 1;
        }

        fun main() {
        	let y: i32 = f(true);
        	print(y);
        }
        "#,
        "1\n",
    );
}

// Unchanged by the amendment: no `ret`, the tail decides.
#[test]
fn b126_a_tail_only_body_still_infers_from_its_tail() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun f() {
        	5
        }

        fun main() {
        	let y: i32 = f();
        	print(y);
        }
        "#,
        "5\n",
    );
}

// A reachable tail and a `ret` that agree — both paths run.
#[test]
fn b126_a_ret_and_a_tail_that_agree_infer_one_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun f(x: bool) {
        	if x {
        		ret 1;
        	}
        	2
        }

        fun main() {
        	let y: i32 = f(true);
        	print(y);
        	print(f(false));
        }
        "#,
        "1\n2\n",
    );
}

// The disagreement is refused at the `ret`, naming both types and the tail it
// is measured against, with a note at that tail — and it is the ONLY
// diagnostic: the function's calls type as `any` afterwards, so the old
// `Expected i32, but got void` never appears beside it (B5).
#[test]
fn b126_a_ret_disagreeing_with_the_tail_is_refused_at_the_ret() {
    let source = r#"
        fun f(x: bool) {
        	if x {
        		ret "s";
        	}
        	2
        }

        fun main() {
        	let y: i32 = f(true);
        }
        "#;
    let head = "this `ret` returns str, but the function's return type is inferred as i32 from its tail; make every return agree, or declare the return type";
    assert_fails_spanning(source, "ret \"s\"", head);
    assert_fails_once_with(source, "return type is inferred");
    assert_fails_noting(source, head, "2", "the tail it is inferred from");
    assert_fails_without(source, "Expected");
}

// A bare `ret` is `ret <void>` (rule 2's reading): it disagrees with a value
// tail. Before the amendment `f(true)` handed back `undefined` under `i32`.
#[test]
fn b126_a_bare_ret_in_a_value_tailed_function_is_refused() {
    let source = r#"
        fun f(x: bool) {
        	if x {
        		ret;
        	}
        	2
        }

        fun main() {
        	let y: i32 = f(true);
        }
        "#;
    assert_fails_spanning(
        source,
        "ret",
        "a bare `ret` returns nothing, but the function's return type is inferred as i32 from its tail; return a value, or declare the return type",
    );
    assert_fails_without(source, "Expected");
}

// The tail the body CAN reach is evidence even when it is the parser's
// synthesized void after a last statement that does not leave: `f(false)`
// falls through, so typing the function `i32` from its `ret` would be the
// `undefined`-under-`i32` miscompile one layer up. The origin is named as the
// body ending without a value, noted at the closing brace.
#[test]
fn b126_a_value_ret_beside_a_fall_through_is_refused() {
    let source = r#"
        import std::io::print;

        fun f(x: bool) {
        	if x {
        		ret 1;
        	}
        	print("fell through");
        }

        fun main() {
        	let y: i32 = f(false);
        }
        "#;
    let head = "this `ret` returns i32, but the function's return type is inferred as void from its body ending without a value; make every return agree, or declare the return type";
    assert_fails_spanning(source, "ret 1", head);
    assert_fails_noting_nth(source, head, "}", 1, "the body ends here without a value");
    assert_fails_without(source, "Expected");
}

// The other fall-through spelling: the tail IS an `if` with no `else`, which
// produces void on the path that takes no branch (S3's regime 2, named the
// same way here).
#[test]
fn b126_a_value_ret_beside_an_else_less_if_tail_is_refused() {
    let source = r#"
        fun f(x: bool) {
        	if x {
        		ret 1;
        	}
        }

        fun main() {
        	let y: i32 = f(false);
        }
        "#;
    assert_fails_spanning(
        source,
        "ret 1",
        "this `ret` returns i32, but the function's return type is inferred as void from its tail, an `if` with no `else`; make every return agree, or declare the return type",
    );
    assert_fails_noting(
        source,
        "an `if` with no `else`",
        "if x {\n        \t\tret 1;\n        \t}",
        "an `if` with no `else` produces void",
    );
}

// A `ret` of a void call beside a value tail is the same disagreement, read
// the other way round (rule 2's "a void call ret is not a value return").
#[test]
fn b126_a_void_ret_beside_a_value_tail_is_refused() {
    assert_fails_with(
        r#"
        import std::io::print;

        fun f(x: bool) {
        	if x {
        		ret print("x");
        	}
        	1
        }

        fun main() {
        	let y: i32 = f(false);
        }
        "#,
        "this `ret` returns void, but the function's return type is inferred as i32 from its tail",
    );
}

// Several `ret`s and no reachable tail: they agree, and every path runs.
#[test]
fn b126_rets_that_agree_infer_one_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun sign(x: i32) {
        	if x > 0 {
        		ret 1;
        	}
        	if x < 0 {
        		ret -1;
        	}
        	ret 0;
        }

        fun main() {
        	let y: i32 = sign(5);
        	print(y);
        	print(sign(-3));
        	print(sign(0));
        }
        "#,
        "1\n-1\n0\n",
    );
}

// With no reachable tail the first `ret` sets the type and a later one that
// disagrees is refused at ITSELF, naming the earlier `ret` as the origin; one
// refusal, no cascade at the call.
#[test]
fn b126_rets_that_disagree_are_refused_at_the_later_ret() {
    let source = r#"
        fun f(x: bool) {
        	if x {
        		ret 1;
        	}
        	ret "s";
        }

        fun main() {
        	let y: i32 = f(true);
        }
        "#;
    let head = "this `ret` returns str, but the function's return type is inferred as i32 from an earlier `ret`; make every return agree, or declare the return type";
    assert_fails_spanning(source, "ret \"s\"", head);
    assert_fails_noting(
        source,
        head,
        "ret 1",
        "the earlier `ret` it is inferred from",
    );
    assert_fails_once_with(source, "return type is inferred");
    assert_fails_without(source, "Expected");
}

// A generic return-position call in a `ret` is read WITH the tail's type as
// its expectation, so `List::new()` binds its element from the tail.
#[test]
fn b126_a_ret_of_a_generic_call_binds_from_the_tail() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::list::List;

        fun make(flag: bool) {
        	if flag {
        		ret List::new();
        	}
        	mut items = List::new();
        	items.push(1);
        	items
        }

        fun main() {
        	let xs: List<i32> = make(false);
        	print(xs.len());
        	let empty: List<i32> = make(true);
        	print(empty.len());
        }
        "#,
        "1\n0\n",
    );
}

// A generic function's `ret` of its own parameter agrees with a tail of the
// same parameter type, at every instantiation.
#[test]
fn b126_a_generic_function_infers_from_a_ret_of_its_parameter() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun pick<T>(flag: bool, a: T, b: T) {
        	if flag {
        		ret a;
        	}
        	b
        }

        fun main() {
        	let y: i32 = pick(true, 1, 2);
        	print(y);
        	let s: str = pick(false, "a", "b");
        	print(s);
        }
        "#,
        "1\nb\n",
    );
}

// A closure inside an unannotated function keeps its own frame: its `ret`s
// type the closure (rule 4), the function's `ret`s type the function, and the
// two may differ. The negative half: a closure's bad `ret` is the CLOSURE's
// diagnostic, never the function's.
#[test]
fn b126_a_nested_closures_rets_stay_on_the_closures_frame() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun label(g: |i32| str): str {
        	g(10)
        }

        fun outer(x: bool) {
        	let text = label(|v| {
        		if v > 5 {
        			ret "big";
        		}
        		"small"
        	});
        	if x {
        		ret text.len();
        	}
        	ret 0;
        }

        fun main() {
        	let y: i32 = outer(true);
        	print(y);
        }
        "#,
        "3\n",
    );
    let source = r#"
        fun apply(g: |i32| i32): i32 {
        	g(10)
        }

        fun outer(x: bool) {
        	let inner = apply(|v| {
        		if v > 5 {
        			ret "str";
        		}
        		v
        	});
        	if x {
        		ret inner;
        	}
        	ret 0;
        }

        fun main() {
        	let y: i32 = outer(true);
        }
        "#;
    assert_fails_with(
        source,
        "this `ret` returns str, but the closure's body yields i32",
    );
    assert_fails_without(source, "the function's return type is inferred");
}

// Recursion: a self-call contributes nothing (its type IS the answer under
// construction), so the OTHER returns decide — through a `ret` of an
// expression over the self-call, a tail over it, and a `ret` that is nothing
// but the self-call (which used to be "could not be resolved").
#[test]
fn b126_a_recursive_unannotated_function_infers_from_its_other_returns() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun count(n: i32) {
        	if n == 0 {
        		ret 0;
        	}
        	ret 1 + count(n - 1);
        }

        fun fact(n: i32) {
        	if n <= 1 {
        		ret 1;
        	}
        	n * fact(n - 1)
        }

        fun down(n: i32) {
        	if n == 0 {
        		ret 0;
        	}
        	ret down(n - 1);
        }

        fun main() {
        	let a: i32 = count(3);
        	print(a);
        	let b: i32 = fact(5);
        	print(b);
        	let c: i32 = down(4);
        	print(c);
        }
        "#,
        "3\n120\n0\n",
    );
}

// Mutual recursion: each function's answer is built on the other's, and the
// one built on an unfinished neighbour is not recorded — each computes
// top-level through its own constraint, so both coerce and both call.
#[test]
fn b126_mutually_recursive_unannotated_functions_infer_together() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun a(n: i32) {
        	if n == 0 {
        		ret 0;
        	}
        	b(n - 1)
        }

        fun b(n: i32) {
        	a(n - 1)
        }

        fun main() {
        	let y: i32 = a(4);
        	print(y);
        	let z: i32 = b(3);
        	print(z);
        }
        "#,
        "0\n0\n",
    );
}

// B126 residue (2026-08-22), KNOWN, NOT FIXED: a self-call bound by a `let`
// and read in the tail. The inference path does not read a `let` binding
// through its initializer, so the tail `x + 1` is unresolved while `x`'s own
// constraint is waiting on `g(n - 1)` — and the function's answer never
// lands: "type of variable 'x' could not be resolved". Same on `next` before
// the amendment. Asserts what SHOULD hold; goes green when the binding is
// read through its initializer on the inference path.
#[test]
#[ignore = "B191 (B126's residue, re-owned 2026-09-01): a self-call bound by a `let` and read in the tail \
            (`let x = g(n - 1); x + 1`) in an unannotated recursive body still fails \
            \"could not be resolved\" — a `let` binding is not read through its \
            initializer on the inference path"]
fn b126_a_let_bound_self_call_read_in_the_tail_resolves() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun g(n: i32) {
        	if n == 0 {
        		ret 1;
        	}
        	let x = g(n - 1);
        	x + 1
        }

        fun main() {
        	let y: i32 = g(3);
        	print(y);
        }
        "#,
        "4\n",
    );
}

// A function whose only return evidence is itself never returns: `never`,
// which satisfies any expectation (as `panic(..)` does). It used to be
// "could not be resolved".
#[test]
fn b126_a_function_that_only_calls_itself_is_never() {
    assert_compiles(
        r#"
        fun forever(n: i32) {
        	forever(n - 1)
        }

        fun main() {
        	if false {
        		let y: i32 = forever(5);
        	}
        }
        "#,
    );
}

// The conformance reader sees the unified type: an unannotated impl member
// that leaves by `ret` conforms when the `ret` agrees with the trait, and is
// refused when it does not (it used to pass leniently and print "wide").
#[test]
fn b126_an_unannotated_impl_method_conforms_by_its_unified_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        trait Shape {
        	fun area(self): i32;
        }

        struct Sq { s: i32 }

        impl Sq with Shape {
        	fun area(self) {
        		ret self.s * self.s;
        	}
        }

        fun main() {
        	let q = Sq { s = 3 };
        	print(q.area());
        }
        "#,
        "9\n",
    );
    assert_fails_with(
        r#"
        import std::io::print;

        trait Shape {
        	fun area(self): i32;
        }

        struct Sq { s: i32 }

        impl Sq with Shape {
        	fun area(self) {
        		ret "wide";
        	}
        }

        fun main() {
        	let q = Sq { s = 3 };
        	print(q.area());
        }
        "#,
        "`Sq`'s `area` returns `str`, but `Shape` declares `i32`",
    );
}

// The rule is the function's, not the shape's: an `async fun` without an
// annotation infers the same way, and an awaited call yields that type.
#[test]
fn b126_an_async_function_without_annotation_infers_from_its_rets() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        async fun f(x: bool) {
        	ret 1;
        }

        async fun g(x: bool) {
        	if x {
        		ret 1;
        	}
        	2
        }

        async fun main() {
        	let y: i32 = f(true);
        	print(y);
        	let z: i32 = g(true);
        	print(z);
        	print(g(false));
        }
        "#,
        "1\n1\n2\n",
    );
}

// The `for` protocol's reader (B92) goes through the same helper: an
// unannotated `next` that leaves by `ret Some(..)`/`ret None` drives the loop
// (it used to be refused as yielding `void`), and one whose `ret` yields a
// non-`Option` is refused by B92's own message with the unified type.
#[test]
fn b126_an_unannotated_next_that_leaves_by_ret_drives_the_loop() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        struct Two { at: i32 }

        impl Two {
        	fun next(&mut self) {
        		if self.at >= 2 {
        			ret None;
        		}
        		self.at += 1;
        		ret Some(self.at);
        	}
        }

        fun main() {
        	mut two = Two { at = 0 };
        	for item in two {
        		print(item);
        	}
        	print(9);
        }
        "#,
        "1\n2\n9\n",
    );
    assert_fails_with(
        r#"
        import std::io::print;

        struct Num { at: i32 }

        impl Num {
        	fun next(&mut self) {
        		self.at += 1;
        		ret self.at;
        	}
        }

        fun main() {
        	mut num = Num { at = 0 };
        	for item in num {
        		print(item);
        	}
        }
        "#,
        "its `next` is unannotated and its body yields `i32`",
    );
}

// Closure coercion (B20) reads the helper on both of its paths — the
// inferring one in `reconcile_type` and the recorded one in `compare_type` —
// so a `ret`-only function fits a `|bool| i32` slot (it used to be refused as
// `fn say(bool)` against `|bool| i32`).
#[test]
fn b126_a_ret_only_function_coerces_to_a_closure_slot() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun run(f: |bool| i32) {
        	print(f(true));
        }

        fun say(x: bool) {
        	ret 1;
        }

        fun main() {
        	run(say);
        }
        "#,
        "1\n",
    );
}

// An exhaustive `if`/`else` of `ret`s in tail position types `never` (B124),
// which is no evidence; the `ret`s decide. Before, the function itself was
// `never` and `let y: str = f(false)` compiled and printed `2`.
#[test]
fn b126_an_exhaustive_if_else_of_rets_infers_from_the_rets() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun f(x: bool) {
        	if x {
        		ret 1;
        	} else {
        		ret 2;
        	}
        }

        fun main() {
        	let y: i32 = f(false);
        	print(y);
        }
        "#,
        "2\n",
    );
    assert_fails_with(
        r#"
        import std::io::print;

        fun f(x: bool) {
        	if x {
        		ret 1;
        	} else {
        		ret 2;
        	}
        }

        fun main() {
        	let y: str = f(false);
        	print(y);
        }
        "#,
        "Expected str, but got i32 instead.",
    );
}

// --- S3: the missing return value re-anchored (editing-dx.md §3) ----------
// The known-bad shape the charter named first: a missing return value used
// to underline the WHOLE closure (or, worse, the whole call) rather than the
// gap. Three regimes, per §3.2-§3.4; pins below are named after the paper's
// probes (P21-P28).

// P22 — regime 1: a named function whose body ends in a non-expression
// statement (here a `let`) has NO tail to anchor at all. Old behavior: a
// zero-width point one byte PAST the closing brace (invisible in an editor,
// §3.2). New: the closing brace itself, one character wide.
#[test]
fn missing_return_value_regime_1_anchors_the_closing_brace() {
    assert_fails_spanning(
        r#"
        fun total(a: i32, b: i32): i32 {
        	let sum: i32 = a + b;
        }

        fun main() {
        	total(1, 2);
        }
        "#,
        "}",
        "Expected i32, but got void instead: this body ends without producing a value.",
    );
}

// Regime 1' for a NAMED function (not one of the paper's own probes — P24's
// closure shape is the paper's example, but the same statement-based
// distinction applies to a plain function body, and CLAUDE.md's "per case,
// not per example" wants the edge covered directly): the last statement IS
// an expression whose value would satisfy the return type, discarded only by
// its trailing `;` — the sharper, actionable message.
#[test]
fn missing_return_value_regime_1_prime_named_function_names_the_semicolon() {
    assert_fails_spanning(
        r#"
        fun total(a: i32, b: i32): i32 {
        	a + b;
        }

        fun main() {
        	total(1, 2);
        }
        "#,
        "}",
        "Expected i32, but got void instead: the `;` discards this body's last value.",
    );
}

// A last statement that reconciles by NAME-COINCIDENCE only, not because
// removing its `;` would make it the tail: `let` is a declaration, not an
// expression, even though its own type (i32) happens to match the return
// type. Regression guard for the bug this shape reproduced during
// development (a bare `self.variables` miss let a `let`'s binding type stand
// in for "the last statement's value").
#[test]
fn a_declaration_as_the_last_statement_does_not_trigger_regime_1_prime() {
    assert_fails_with(
        r#"
        fun total(a: i32, b: i32): i32 {
        	let sum: i32 = a + b;
        }

        fun main() {
        	total(1, 2);
        }
        "#,
        "this body ends without producing a value.",
    );
}

// P25 — regime 2: an `if` with no `else` in tail position is a REAL
// expression (not the parser's synthesized `Void`), so its span was already
// A1-compliant (§3.3, unchanged) — what §16 deferred as "needs a provenance
// channel" and §17 builds: the WORDING now names the missing `else` as the
// gap (`if_branch_has_final_else` asked again at the diagnostic site,
// editing-dx.md §17.1) instead of the generic mismatch phrasing regimes 1/1'
// no longer use for their own gap.
#[test]
fn missing_return_value_regime_2_if_with_no_else_names_the_gap() {
    assert_fails_with(
        r#"
        fun classify(n: i32): str {
        	if n > 0 {
        		"positive"
        	}
        }

        fun main() {
        	classify(1);
        }
        "#,
        "Expected str, but got void instead: an `if` with no `else` produces void.",
    );
}

// Regression guard for the refinement's own boundary: a void-typed CALL in
// tail position (no trailing `;` — that would be regime 1, §3.2) is a real
// value the body produced too, but it is not an `if`, so it must keep the
// plain mismatch message rather than being swept into regime 2's wording.
#[test]
fn missing_return_value_a_void_call_tail_keeps_the_generic_message() {
    assert_fails_with(
        r#"
        import std::io::print;

        fun classify(n: i32): str {
        	print(n)
        }

        fun main() {
        	classify(1);
        }
        "#,
        "Expected str, but got void instead.",
    );
}

// P23 — regime 3, unannotated closure bound to an annotated `let`: the
// closure's OWN parameters need no annotation (`|value|`, filled by
// bidirectional inference from `|i32| i32`); once they reconcile, the body
// checks in RETURN POSITION against the annotation's return half instead of
// the whole closure value being compared at the `let` — the closing BRACE,
// not the closure's `|params| { .. }` span, `points.map(..)`'s whole call,
// or anything else upstream.
#[test]
fn missing_return_value_regime_3_context_closure_anchors_its_own_brace() {
    assert_fails_spanning(
        r#"
        fun main() {
        	let scale: |i32| i32 = |value| {
        		let doubled: i32 = value * 2;
        	};
        }
        "#,
        "}",
        "Expected i32, but got void instead: this body ends without producing a value.",
    );
}

// P24 — the one-line spelling of the same shape: the whole mistake is one
// character (`;`), and the message now names it. Old behavior: 22 characters
// underlined (the whole closure) to ask for one to be deleted.
#[test]
fn missing_return_value_regime_3_context_closure_one_liner_names_the_semicolon() {
    assert_fails_spanning(
        r#"
        fun main() {
        	let scale: |i32| i32 = |value| { value * 2; };
        }
        "#,
        "}",
        "Expected i32, but got void instead: the `;` discards this body's last value.",
    );
}

// P26 — the root-cause probe: a closure's OWN return-type annotation
// (`: i32`) used to be parsed, re-printed by the formatter, and completely
// ignored by type checking (`Closure::return_type` had no analyzer reader at
// all). It now gets rule 2 "directly" — the same return-position check a
// named function's declared return type gets — independent of any
// surrounding context.
#[test]
fn missing_return_value_regime_3_annotated_closure_is_newly_checked() {
    assert_fails_spanning(
        r#"
        import std::io::print;

        fun main() {
        	let scale: |i32| i32 = |value: i32|: i32 { print(value); };
        }
        "#,
        "}",
        "Expected i32, but got void instead: this body ends without producing a value.",
    );
}

// An annotated closure whose body actually satisfies its OWN declared
// return type compiles clean — the new check is additive, not a false
// positive on the common case.
#[test]
fn an_annotated_closure_whose_body_satisfies_its_own_return_type_compiles() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
        	let scale: |i32| i32 = |value: i32|: i32 { value * 2 };
        	print(scale(5));
        }
        "#,
        "10\n",
    );
}

// P27 — the contrast that bounds the fix: when the PARAMETER type is what
// differs (not the return type), the whole-closure anchor is correct and
// stays untouched — nothing narrower would be honest about a closure that,
// as written, is the wrong VALUE.
#[test]
fn missing_return_value_regime_3_parameter_mismatch_keeps_the_whole_value_anchor() {
    assert_fails_spanning(
        r#"
        fun main() {
        	let scale: |i32| i32 = |value: str| { 1 };
        }
        "#,
        "|value: str| { 1 }",
        "Expected |i32| i32, but got |str| i32 instead.",
    );
}

// P21 — the generic-binding case, closed by B125 (type-solver.md "The
// expectation is an input of generic call resolution"): `map<U>`'s `U` is
// bound from the call site's EXPECTATION (`let widths: List<i32>`) before the
// closure argument is typed, so the closure arm's return-position check has
// a ground target on the first attempt and reports at the closure's own
// brace — not one level out as `List<void>` against the annotation. The
// `type_is_ground` gate is unchanged: it still declines a target nobody has
// bound; the expectation is what binds it now.
#[test]
fn missing_return_value_regime_3_through_a_generic_binding() {
    assert_fails_spanning_nth(
        r#"
        import std::io::print;

        struct Point { x: i32, y: i32 }

        fun main() {
        	mut points: List<Point> = List::new();
        	points.push(Point { x = 1, y = 10 });
        	let widths: List<i32> = points.map(|point| {
        		point.x * 2;
        	});
        	print(widths.len());
        }
        "#,
        "}",
        2,
        "Expected i32, but got void instead: the `;` discards this body's last value.",
    );
}

// The same binding through a declared return type's TAIL — the expectation
// the function walk seeds for its body tail reaches the call at priority 6
// exactly as the `let` annotation does.
#[test]
fn missing_return_value_regime_3_through_a_generic_binding_in_tail_position() {
    assert_fails_spanning_nth(
        r#"
        import std::io::print;

        struct Point { x: i32, y: i32 }

        fun widths(points: List<Point>): List<i32> {
        	points.map(|point| {
        		point.x * 2;
        	})
        }

        fun main() {
        	mut points: List<Point> = List::new();
        	points.push(Point { x = 1, y = 10 });
        	print(widths(points).len());
        }
        "#,
        "}",
        1,
        "Expected i32, but got void instead: the `;` discards this body's last value.",
    );
}

// And through a `ret` — the third walk-time expectation seed.
#[test]
fn missing_return_value_regime_3_through_a_generic_binding_in_ret_position() {
    assert_fails_spanning_nth(
        r#"
        import std::io::print;

        struct Point { x: i32, y: i32 }

        fun widths(points: List<Point>): List<i32> {
        	ret points.map(|point| {
        		point.x * 2;
        	});
        }

        fun main() {
        	mut points: List<Point> = List::new();
        	points.push(Point { x = 1, y = 10 });
        	print(widths(points).len());
        }
        "#,
        "}",
        1,
        "Expected i32, but got void instead: the `;` discards this body's last value.",
    );
}

// The free-function path rides the same binding source (B90 aligned the two
// call paths; B125 keeps them aligned): `apply<U>`'s `U` from the `let`.
#[test]
fn missing_return_value_regime_3_through_a_free_functions_generic_binding() {
    assert_fails_spanning_nth(
        r#"
        import std::io::print;

        fun apply<U>(xs: List<i32>, f: |i32| U): List<U> {
        	xs.map(f)
        }

        fun main() {
        	let xs = [1, 2];
        	let ys: List<i32> = apply(xs, |x| {
        		x * 2;
        	});
        	print(ys.len());
        }
        "#,
        "}",
        1,
        "Expected i32, but got void instead: the `;` discards this body's last value.",
    );
}

// `Signal::map<U>` — the shape the todo example annotates around; `sync |T|
// U` binds the same way as `List::map`'s `|T| U`.
#[test]
fn missing_return_value_regime_3_through_a_signal_maps_generic_binding() {
    assert_fails_spanning_nth(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, SignalCell, owner_scope };

        fun main() {
        	let scope = Owner::new();
        	let n = owner_scope.run(scope, || {
        		let count = Signal::new(1);
        		let doubled: SignalCell<i32> = count.map(|n| {
        			n * 2;
        		});
        		doubled.get()
        	});
        	print(n);
        	scope.dispose();
        }
        "#,
        "}",
        1,
        "Expected i32, but got void instead: the `;` discards this body's last value.",
    );
}

// The nested shapes: the expectation reaches a call standing in a block
// tail, a value-`if`'s branch tail, or a match leg — seeded at WALK time
// through the syntactic tails (`seed_tail_expectations`), and by
// `resolve_match` before its subject can defer the attempt.
#[test]
fn missing_return_value_regime_3_through_a_generic_binding_in_a_block_tail() {
    let source = r#"
        import std::io::print;

        struct Point { x: i32, y: i32 }

        fun main() {
        	mut points: List<Point> = List::new();
        	points.push(Point { x = 1, y = 10 });
        	let widths: List<i32> = {
        		points.map(|point| {
        			point.x * 2;
        		})
        	};
        	print(widths.len());
        }
        "#;
    assert_fails_spanning_nth(
        source,
        "}",
        2,
        "Expected i32, but got void instead: the `;` discards this body's last value.",
    );
    assert_fails_without(source, "List<void>");
}

#[test]
fn missing_return_value_regime_3_through_a_generic_binding_in_an_if_branch() {
    let source = r#"
        import std::io::print;

        struct Point { x: i32, y: i32 }

        fun main() {
        	mut points: List<Point> = List::new();
        	points.push(Point { x = 1, y = 10 });
        	let widths: List<i32> = if points.len() > 0 {
        		points.map(|point| {
        			point.x * 2;
        		})
        	} else {
        		[]
        	};
        	print(widths.len());
        }
        "#;
    assert_fails_spanning_nth(
        source,
        "}",
        2,
        "Expected i32, but got void instead: the `;` discards this body's last value.",
    );
    assert_fails_without(source, "List<void>");
}

// A match whose SUBJECT is itself a call: the legs used to be seeded only
// once the subject landed (a pass after the leg's call had committed).
#[test]
fn missing_return_value_regime_3_through_a_generic_binding_in_a_match_leg() {
    let source = r#"
        import std::io::print;

        struct Point { x: i32, y: i32 }

        fun main() {
        	mut points: List<Point> = List::new();
        	points.push(Point { x = 1, y = 10 });
        	let widths: List<i32> = match points.len() {
        		0 => [],
        		_ => points.map(|point| {
        			point.x * 2;
        		}),
        	};
        	print(widths.len());
        }
        "#;
    assert_fails_spanning_nth(
        source,
        "}",
        2,
        "Expected i32, but got void instead: the `;` discards this body's last value.",
    );
    assert_fails_without(source, "List<void>");
}

// --- B125's B5 set: when the closure's tail, the annotation and the receiver
// disagree in different combinations, exactly ONE diagnostic fires, and the
// value-position reconcile at the `let` never doubles it — the closure's
// reported type is the target it was held to (S3's rule), so the call types
// as the annotation says and the `let` has nothing to add.

// The void tail under an annotation: one report, at the brace, and the old
// whole-call `List<void>` message is gone rather than joined.
#[test]
fn b125_a_void_closure_tail_under_an_annotation_reports_once_at_the_brace() {
    let source = r#"
        import std::io::print;

        struct Point { x: i32, y: i32 }

        fun main() {
        	mut points: List<Point> = List::new();
        	points.push(Point { x = 1, y = 10 });
        	let widths: List<i32> = points.map(|point| {
        		point.x * 2;
        	});
        	print(widths.len());
        }
        "#;
    assert_fails_once_with(source, "Expected");
    assert_fails_without(source, "List<void>");
}

// A block tail that produces the WRONG value (i32 where the annotation's `U`
// is str): one report, at the brace, in the annotation's terms.
#[test]
fn b125_a_closure_tail_disagreeing_with_the_annotation_reports_once_at_the_brace() {
    let source = r#"
        import std::io::print;

        struct Point { x: i32, y: i32 }

        fun main() {
        	mut points: List<Point> = List::new();
        	points.push(Point { x = 1, y = 10 });
        	let widths: List<str> = points.map(|point| {
        		point.x * 2
        	});
        	print(widths.len());
        }
        "#;
    assert_fails_once_with(source, "Expected");
    assert_fails_spanning_nth(source, "}", 2, "Expected str, but got i32 instead.");
    assert_fails_without(source, "List<str>");
}

// The bare-expression spelling: S3's route used to be scoped to block bodies
// ("no closing brace to anchor at"), so this reported the closure as a whole
// value at the argument check. B132 routes bare bodies through the same
// return-position check, anchored ON the expression — the b125 B5 claim
// (exactly one diagnostic, the `let` never doubles it) is unchanged; only
// the anchor narrowed. Re-pinned from
// `b125_a_bare_closure_disagreeing_with_the_annotation_reports_once_at_the_closure`:
// same program, the new anchor.
#[test]
fn b132_a_bare_closure_body_disagreeing_with_the_annotation_reports_on_the_expression() {
    let source = r#"
        import std::io::print;

        struct Point { x: i32, y: i32 }

        fun main() {
        	mut points: List<Point> = List::new();
        	points.push(Point { x = 1, y = 10 });
        	let widths: List<str> = points.map(|point| point.x * 2);
        	print(widths.len());
        }
        "#;
    assert_fails_once_with(source, "Expected");
    assert_fails_spanning(source, "point.x * 2", "Expected str, but got i32 instead.");
    assert_fails_without(source, "|Point| str");
    assert_fails_without(source, "List<str>");
}

// The void-valued bare body: the same route, the same anchor, the plain
// mismatch wording (a real void value, not the missing-value regime — there
// is no `;` to blame in a bare expression).
#[test]
fn b132_a_void_bare_closure_body_reports_on_the_expression() {
    let source = r#"
        import std::io::print;

        struct Point { x: i32, y: i32 }

        fun main() {
        	mut points: List<Point> = List::new();
        	points.push(Point { x = 1, y = 10 });
        	let widths: List<i32> = points.map(|point| print(point.x));
        	print(widths.len());
        }
        "#;
    assert_fails_once_with(source, "Expected");
    assert_fails_spanning(
        source,
        "print(point.x)",
        "Expected i32, but got void instead.",
    );
    assert_fails_without(source, "List<void>");
}

// A bare `if` with no `else` gets regime 2's wording (S3), exactly as the
// same tail inside a block body does.
#[test]
fn b132_a_bare_if_without_else_body_names_the_gap() {
    let source = r#"
        import std::io::print;

        struct Point { x: i32, y: i32 }

        fun main() {
        	mut points: List<Point> = List::new();
        	points.push(Point { x = 1, y = 10 });
        	let widths: List<i32> = points.map(|point| if point.x > 0 { 1 });
        	print(widths.len());
        }
        "#;
    assert_fails_once_with(source, "Expected");
    assert_fails_spanning(
        source,
        "if point.x > 0 { 1 }",
        "Expected i32, but got void instead: an `if` with no `else` produces void.",
    );
}

// The free-function spelling shares the route (B90 keeps the two call paths
// one rule): the expectation binds `U`, and the bare body reports on the
// expression there too.
#[test]
fn b132_the_free_function_spelling_reports_on_the_expression() {
    let source = r#"
        import std::io::print;

        fun apply<U>(xs: List<i32>, f: |i32| U): List<U> {
        	xs.map(f)
        }

        fun main() {
        	let xs = [1, 2];
        	let out: List<str> = apply(xs, |x| x * 2);
        	print(out.len());
        }
        "#;
    assert_fails_once_with(source, "Expected");
    assert_fails_spanning(source, "x * 2", "Expected str, but got i32 instead.");
    assert_fails_without(source, "List<str>");
}

// A closure's OWN return annotation over a bare body takes the same route
// (rule 2 "directly"): the report lands on the expression, not the closure
// as a whole value.
#[test]
fn b132_an_annotated_bare_body_reports_on_the_expression() {
    let source = r#"
        import std::io::print;

        fun main() {
        	let f = |x: i32|: str x + 1;
        	print(f(1));
        }
        "#;
    assert_fails_once_with(source, "Expected");
    assert_fails_spanning(source, "x + 1", "Expected str, but got i32 instead.");
}

// The route must not manufacture failures: an agreeing bare body under the
// same expectation still compiles and runs.
#[test]
fn b132_an_agreeing_bare_body_still_compiles_and_runs() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Point { x: i32, y: i32 }

        fun main() {
        	mut points: List<Point> = List::new();
        	points.push(Point { x = 1, y = 10 });
        	let widths: List<i32> = points.map(|point| point.x * 2);
        	print(widths[0]);
        }
        "#,
        "2\n",
    );
}

// --- B133: rule 4 lifted to the reachable-tail rule (ret-checking.md rule 4
// as amended). A closure's return type is the unification of its REACHABLE
// tail and every `ret` — the same evidence, through the same fold, an
// unannotated function uses (B126) — so `{ ret 1; }` infers in a closure
// exactly as it does in a function, and the dead synthesized-void tail is no
// longer a void vote against its own `ret`s. The conservative "make the
// ret'd value the body's tail" steer survives exactly where the genuine
// disagreement remains: a value-`ret` beside a body path that yields no
// value.

// The headline: a `ret`-only closure body infers its return type, and every
// reader agrees — a direct call, and a typed binding of the result.
#[test]
fn b133_a_ret_only_closure_body_infers_its_return_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
        	let f = |x: i32| {
        		ret x * 2;
        	};
        	print(f(3));
        	let y: i32 = f(4);
        	print(y);
        }
        "#,
        "6\n8\n",
    );
}

// A closure that leaves only by `ret` can bind a caller's return-position
// generic bottom-up (the `from_fn` family's shape): the rets ARE the
// closure's return evidence.
#[test]
fn b133_a_closure_ret_binds_a_callers_return_generic() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun apply<U>(f: || U): U {
        	f()
        }

        fun main() {
        	let n = apply(|| {
        		ret 5;
        	});
        	print(n + 1);
        }
        "#,
        "6\n",
    );
}

// A dead tail under a known target: the `ret`s are the only return
// positions, and they check against the target — one refusal, at the `ret`,
// and the pre-lift steer (which blamed the dead tail's synthesized void) is
// gone.
#[test]
fn b133_a_dead_tail_ret_is_checked_against_the_target() {
    let source = r#"
        import std::io::print;

        fun run(f: |i32| i32): i32 {
        	f(1)
        }

        fun main() {
        	let out = run(|value| {
        		ret "s";
        	});
        	print(out);
        }
        "#;
    assert_fails_spanning(
        source,
        "ret \"s\"",
        "this `ret` returns str, but the closure's body yields i32",
    );
    assert_fails_once_with(source, "this `ret` returns");
    assert_fails_without(source, "make the ret'd value the body's tail");
    assert_fails_without(source, "Expected");
}

// B125 interplay, the B5 probe extended to `ret`s: under an annotated `let`
// the expectation binds `U` before the closure is typed, and a dead-tail
// `ret` that disagrees reports ONCE, at the `ret`, in the expectation's
// terms — the `let` and the call add nothing.
#[test]
fn b133_a_dead_tail_ret_reports_once_under_an_expectation() {
    let source = r#"
        import std::io::print;

        struct Point { x: i32, y: i32 }

        fun main() {
        	mut points: List<Point> = List::new();
        	points.push(Point { x = 1, y = 10 });
        	let widths: List<str> = points.map(|point| {
        		ret point.x * 2;
        	});
        	print(widths.len());
        }
        "#;
    assert_fails_spanning(
        source,
        "ret point.x * 2",
        "this `ret` returns i32, but the closure's body yields str",
    );
    assert_fails_once_with(source, "this `ret` returns");
    assert_fails_without(source, "Expected");
    assert_fails_without(source, "List<str>");
}

// ...and the agreeing spellings bind from the expectation and run: a
// `ret`-only body, and a guard-`ret` beside an agreeing tail.
#[test]
fn b133_an_agreeing_ret_closure_binds_from_the_expectation_and_runs() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Point { x: i32, y: i32 }

        fun main() {
        	mut points: List<Point> = List::new();
        	points.push(Point { x = 1, y = 10 });
        	let widths: List<i32> = points.map(|point| {
        		ret point.x * 2;
        	});
        	print(widths[0]);
        	let guarded = points.map(|point| {
        		if point.x > 100 {
        			ret 0;
        		}
        		point.x + 5
        	});
        	print(guarded[0]);
        }
        "#,
        "2\n6\n",
    );
}

// A return-position generic in a closure `ret` binds from the target, in
// both the mixed (guard-`ret` beside a tail) and the dead-tail shapes.
#[test]
fn b133_a_ret_of_a_generic_call_binds_from_the_target() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun run(f: |i32| List<i32>): List<i32> {
        	f(3)
        }

        fun main() {
        	let out = run(|seed| {
        		if seed < 0 {
        			ret List::new();
        		}
        		mut xs: List<i32> = List::new();
        		xs.push(seed);
        		xs
        	});
        	print(out.len());
        	let dead_tail = run(|seed| {
        		ret List::new();
        	});
        	print(dead_tail.len());
        }
        "#,
        "1\n0\n",
    );
}

// The I5/B19 shape with `ret`s: `from_fn`'s callback target (`|| Option<T>`)
// is abstract, `type_is_ground` declines it, and the rets bind `T`
// bottom-up — a callback that leaves only by `ret` now types (the pre-lift
// rule refused it with the steer).
#[test]
fn b133_a_from_fn_callback_that_leaves_by_ret_types() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::iterator::Iterator;
        import std::option::Option::{ self, Some, None };

        fun main() {
        	mut n = 0;
        	let counter = Iterator::from_fn(|| {
        		n = n + 1;
        		if n > 3 {
        			ret None;
        		}
        		ret Some(n);
        	});
        	print(counter.count());
        }
        "#,
        "3\n",
    );
}

// A value-`ret` beside a REACHABLE fall-through is still the genuine
// disagreement rule 4's steer was written for — kept, now with a note at
// the origin (the same origin vocabulary as a function's refusal).
#[test]
fn b133_a_value_ret_beside_a_reachable_fall_through_keeps_the_steer() {
    let source = r#"
        import std::io::print;

        fun main() {
        	let helper = |x: i32| {
        		if x > 5 {
        			ret 99;
        		}
        		print("small");
        	};
        	helper(1);
        }
        "#;
    let head = "the closure's body ends without a value, but this `ret` returns one; make the ret'd value the body's tail";
    assert_fails_spanning(source, "ret 99", head);
    assert_fails_noting_nth(source, head, "}", 1, "the body ends here without a value");
}

// The other reachable-void spelling: the closure's tail is an `if` with no
// `else`, which produces void on the path that takes no branch.
#[test]
fn b133_a_value_ret_beside_an_else_less_if_tail_keeps_the_steer() {
    let source = r#"
        import std::io::print;

        fun main() {
        	let f = |x: i32| {
        		if x > 5 {
        			ret 99;
        		}
        	};
        	f(1);
        	print("done");
        }
        "#;
    let head = "the closure's body ends without a value, but this `ret` returns one; make the ret'd value the body's tail";
    assert_fails_spanning(source, "ret 99", head);
    assert_fails_noting(
        source,
        head,
        "if x > 5 {\n        \t\t\tret 99;\n        \t\t}",
        "an `if` with no `else` produces void",
    );
}

// A `ret` disagreeing with a REACHABLE tail is refused at the `ret`, noting
// the tail — no target in sight (an unannotated binding, a direct call).
#[test]
fn b133_a_ret_disagreeing_with_the_tail_is_refused_at_the_ret() {
    let source = r#"
        import std::io::print;

        fun main() {
        	let f = |x: bool| {
        		if x {
        			ret "s";
        		}
        		2
        	};
        	f(true);
        	print("done");
        }
        "#;
    let head = "this `ret` returns str, but the closure's body yields i32";
    assert_fails_spanning(source, "ret \"s\"", head);
    assert_fails_noting(source, head, "2", "the tail it disagrees with");
    assert_fails_without(source, "Expected");
}

// Two `ret`s that disagree under a dead tail: one refusal, at the later
// `ret`, noting the earlier one it disagrees with (the function rule's
// source-order reading).
#[test]
fn b133_rets_that_disagree_are_refused_at_the_later_ret() {
    let source = r#"
        import std::io::print;

        fun main() {
        	let f = |x: i32| {
        		if x > 0 {
        			ret 1;
        		}
        		ret "s";
        	};
        	f(1);
        }
        "#;
    let head = "this `ret` returns str, but the closure's body yields i32";
    assert_fails_spanning(source, "ret \"s\"", head);
    assert_fails_once_with(source, "this `ret` returns");
    assert_fails_noting(source, head, "ret 1", "the earlier `ret` it disagrees with");
}

// The no-target bare-`ret` twin: a bare `ret` beside a value tail is
// refused at the `ret` (the existing wording), noting the tail.
#[test]
fn b133_a_bare_ret_beside_a_value_tail_is_still_refused() {
    let source = r#"
        import std::io::print;

        fun main() {
        	let f = |x: i32| {
        		if x > 5 {
        			ret;
        		}
        		x + 1
        	};
        	f(1);
        	print("done");
        }
        "#;
    let head = "a bare `ret` exits a closure whose body yields i32; return a value";
    assert_fails_spanning(source, "ret", head);
    assert_fails_noting(source, head, "x + 1", "the tail it disagrees with");
}

// An `async` block whose body leaves only by `ret` settles with the rets'
// type: `async { ret 1; }` is a task of `i32`, and awaiting it hands the
// value back.
#[test]
fn b133_an_async_block_of_rets_settles_with_their_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
        	let t = async {
        		ret 1;
        	};
        	let n: i32 = await t;
        	print(n);
        }
        "#,
        "1\n",
    );
}

// An `async` block's disagreeing `ret`s are refused the same way a
// closure's are — at the later `ret`, noting the earlier one.
#[test]
fn b133_an_async_blocks_disagreeing_rets_are_refused() {
    let source = r#"
        import std::io::print;

        fun main() {
        	let flag = true;
        	let pending = async {
        		if flag {
        			ret "a";
        		}
        		ret 2;
        	};
        	print("x");
        }
        "#;
    let head = "this `ret` returns i32, but the closure's body yields str";
    assert_fails_spanning(source, "ret 2", head);
    assert_fails_noting(
        source,
        head,
        "ret \"a\"",
        "the earlier `ret` it disagrees with",
    );
}

// A `ret`-only closure that is never called stays quiet (deferred), exactly
// as loosely as a never-called closure types everywhere else — the pre-lift
// rule refused it with the steer, a false positive: the `ret` and the dead
// tail never disagreed.
#[test]
fn b133_a_never_called_ret_closure_stays_quiet() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
        	let f = |x| {
        		ret 1;
        	};
        	print("quiet");
        }
        "#,
        "quiet\n",
    );
}

// A PARAMETER that disagrees with the receiver keeps P27's whole-value anchor
// — the expectation binding adds nothing beside it.
#[test]
fn b125_a_closure_parameter_disagreeing_with_the_receiver_reports_once() {
    let source = r#"
        import std::io::print;

        struct Point { x: i32, y: i32 }

        fun main() {
        	mut points: List<Point> = List::new();
        	points.push(Point { x = 1, y = 10 });
        	let widths: List<i32> = points.map(|point: str| point.len());
        	print(widths.len());
        }
        "#;
    assert_fails_once_with(source, "Expected");
    assert_fails_spanning(
        source,
        "|point: str| point.len()",
        "Expected |Point| i32, but got |str| i32 instead.",
    );
}

// All three disagree (parameter vs receiver, tail vs annotation): one
// report, the whole closure, in the receiver's and the annotation's terms.
#[test]
fn b125_a_closure_disagreeing_with_receiver_and_annotation_reports_once() {
    let source = r#"
        import std::io::print;

        struct Point { x: i32, y: i32 }

        fun main() {
        	mut points: List<Point> = List::new();
        	points.push(Point { x = 1, y = 10 });
        	let widths: List<str> = points.map(|point: i32| {
        		point;
        	});
        	print(widths.len());
        }
        "#;
    assert_fails_once_with(source, "Expected");
    assert_fails_with(source, "Expected |Point| str, but got |i32| void instead.");
}

// Precedence: a generic a NON-closure argument binds is never overridden by
// the expectation — `fold<B>`'s `B` is `i32` from the literal, and the `let`
// reports the mismatch where it always did.
#[test]
fn b125_an_argument_bound_generic_outranks_the_expectation() {
    let source = r#"
        import std::io::print;

        fun main() {
        	let xs = [1, 2];
        	let s: str = xs.fold(0, |acc, x| acc + x);
        	print(s);
        }
        "#;
    assert_fails_once_with(source, "Expected");
    assert_fails_spanning(
        source,
        "xs.fold(0, |acc, x| acc + x)",
        "Expected str, but got i32 instead.",
    );
}

// No expectation, no change: an unannotated `let` still takes the closure's
// bottom-up binding (`List<void>` here, and the program is legal).
#[test]
fn b125_an_unannotated_let_keeps_the_bottom_up_binding() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Point { x: i32, y: i32 }

        fun main() {
        	mut points: List<Point> = List::new();
        	points.push(Point { x = 1, y = 10 });
        	let widths = points.map(|point| {
        		point.x * 2;
        	});
        	print(widths.len());
        }
        "#,
        "1\n",
    );
}

// An expectation naming the ENCLOSING function's generic binds `U = T`; the
// closure's target is then abstract, `type_is_ground` declines it (exactly
// the "don't freeze unbound" case the gate exists for), and the body types
// bottom-up as before.
#[test]
fn b125_an_expectation_naming_the_enclosing_generic_binds_through() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun ident<T>(xs: List<T>): List<T> {
        	xs.map(|x| x)
        }

        fun main() {
        	let ys = ident([1, 2, 3]);
        	print(ys.len());
        }
        "#,
        "3\n",
    );
}

// P28 — the B5 violation §16 left unclosed, closed here (editing-dx.md
// §17.2): a bare `ret` in a value-returning function used to report the
// same root cause twice (once at the `ret`, once at the synthesized void
// tail after it — both correctly anchored since S3's parser fix, which is
// what made the duplicate visible enough to fix). B124 (§17.7) generalized
// the dedup: `check_return_position` asks whether the last statement
// DIVERGES, of which "is a `ret`" is one case, so only the `ret`'s own
// check fires.
#[test]
fn a_bare_ret_no_longer_duplicates_the_synthesized_tail_diagnostic() {
    let source = r#"
        fun total(a: i32): i32 {
        	ret;
        }

        fun main() {
        	total(1);
        }
        "#;
    assert_fails_once_with(
        source,
        "Expected i32, but got void instead: this body ends without producing a value.",
    );
    let ret_span = source.find("ret").map(|start| start..start + "ret".len());
    let diagnostics = failure_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .any(|(_, range)| Some(range.clone()) == ret_span),
        "expected the sole diagnostic at `ret`, not the synthesized tail; got: {diagnostics:#?}"
    );
}

// The same dedup, for a `ret` whose VALUE is wrong rather than missing — a
// second B5 violation the survey never named (found while building the
// fix, same mechanism): before the fix this ALSO doubled, pairing the
// value mismatch at `ret` with a spurious "ends without producing a value"
// at the tail for code that, once the first error is fixed, is complete.
#[test]
fn a_mistyped_ret_value_no_longer_duplicates_the_synthesized_tail_diagnostic() {
    assert_fails_once_with(
        r#"
        fun total(a: i32): i32 {
        	ret "nope";
        }

        fun main() {
        	total(1);
        }
        "#,
        "Expected i32, but got",
    );
}

// --- B124: a branch that LEAVES contributes no tail value (editing-dx.md
// §17.7). Every pin below reproduced `Expected str, but got void instead.`
// against complete code before the fix, because each branch unified on its
// own synthesized void tail rather than on the `ret` that actually left.

// The item's own shape: an exhaustive `if`/`else` where every branch is a
// bare `ret`. The whole `if` is the function's tail, so the false mismatch
// landed on the `if` itself.
#[test]
fn an_exhaustive_if_else_of_bare_rets_is_not_a_missing_return() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun classify(value: i32): str {
        	if value > 0 {
        		ret "positive";
        	} else {
        		ret "non-positive";
        	}
        }

        fun main() {
        	print(classify(1));
        	print(classify(-1));
        }
        "#,
        "positive\nnon-positive\n",
    );
}

// The chained spelling — `else if` legs are branches of the same `if`, and
// each one has to be asked about divergence separately.
#[test]
fn an_if_else_if_else_chain_of_bare_rets_is_not_a_missing_return() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun classify(value: i32): str {
        	if value > 0 {
        		ret "positive";
        	} else if value < 0 {
        		ret "negative";
        	} else {
        		ret "zero";
        	}
        }

        fun main() {
        	print(classify(3));
        	print(classify(-3));
        	print(classify(0));
        }
        "#,
        "positive\nnegative\nzero\n",
    );
}

// Nesting: an outer branch whose own body is an exhaustive `if`/`else` of
// `ret`s leaves too — the divergence question recurses rather than stopping
// at the first level.
#[test]
fn a_nested_if_else_of_bare_rets_is_not_a_missing_return() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun classify(value: i32): str {
        	if value > 0 {
        		if value > 10 {
        			ret "big";
        		} else {
        			ret "small";
        		}
        	} else {
        		ret "non-positive";
        	}
        }

        fun main() {
        	print(classify(20));
        	print(classify(2));
        	print(classify(-2));
        }
        "#,
        "big\nsmall\nnon-positive\n",
    );
}

// A plain block of `ret`s in tail position: no `if` at all, so this one is
// carried by `check_return_position`'s own divergence question rather than
// by the branch merge.
#[test]
fn a_block_of_rets_in_tail_position_is_not_a_missing_return() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun classify(value: i32): str {
        	{
        		ret "positive";
        	}
        }

        fun main() {
        	print(classify(1));
        }
        "#,
        "positive\n",
    );
}

// The `match` spelling of the same mistake: every leg's body is a block
// whose tail is the synthesized void after its `ret`, so the leg
// unification made the whole match `void`.
#[test]
fn a_match_whose_every_leg_rets_is_not_a_missing_return() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun classify(value: i32): str {
        	match value {
        		0 => {
        			ret "zero";
        		},
        		_ => {
        			ret "other";
        		},
        	}
        }

        fun main() {
        	print(classify(0));
        	print(classify(7));
        }
        "#,
        "zero\nother\n",
    );
}

// Mixed: one branch leaves, the other yields. The `if` is the yielded
// type — `Never` yields in `reconcile_type` — where before the fix the
// leaving branch's void won the merge and the whole `if` typed as void.
#[test]
fn an_if_branch_that_rets_beside_one_that_yields_takes_the_yielded_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun classify(value: i32): str {
        	if value > 0 {
        		ret "positive";
        	} else {
        		"non-positive"
        	}
        }

        fun main() {
        	print(classify(1));
        	print(classify(-1));
        }
        "#,
        "positive\nnon-positive\n",
    );
}

// The `match` twin of the mixed shape, which before the fix reported the
// missing return AND a bogus `match legs have mismatched types: expected
// void, but got str` on the leg that was right.
#[test]
fn a_match_leg_that_rets_beside_one_that_yields_takes_the_yielded_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun classify(value: i32): str {
        	match value {
        		0 => {
        			ret "zero";
        		},
        		_ => "other",
        	}
        }

        fun main() {
        	print(classify(0));
        	print(classify(7));
        }
        "#,
        "zero\nother\n",
    );
}

// The same merge in VALUE position rather than return position: a `let`
// bound to an `if` one of whose branches leaves the enclosing function.
#[test]
fn a_diverging_if_branch_in_a_let_takes_the_live_branchs_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun classify(value: i32): str {
        	let label = if value > 0 {
        		ret "early";
        	} else {
        		"other"
        	};
        	label
        }

        fun main() {
        	print(classify(1));
        	print(classify(-1));
        }
        "#,
        "early\nother\n",
    );
}

// The STATEMENT spelling (a trailing `;` on the `if`), where the body's
// tail is the parser's synthesized void rather than the `if` itself: the
// tail is dead code after a last statement that leaves, so holding it to
// the declared type was §17.2's `ret`-only dedup missing its general case.
#[test]
fn a_last_statement_if_that_diverges_leaves_no_tail_to_check() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun classify(value: i32): str {
        	let doubled = value * 2;
        	if doubled > 0 {
        		ret "positive";
        	} else {
        		ret "non-positive";
        	};
        }

        fun main() {
        	print(classify(1));
        	print(classify(-1));
        }
        "#,
        "positive\nnon-positive\n",
    );
}

// The same, for a `match` — the shape that forced the check to resolve
// time: at walk time a `match` is not yet in `expr_id_to_expr_map` (it is
// inserted by `resolve_match`), so a walk-time divergence question cannot
// see this one leave at all.
#[test]
fn a_last_statement_match_that_diverges_leaves_no_tail_to_check() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun classify(value: i32): str {
        	match value {
        		0 => {
        			ret "zero";
        		},
        		_ => {
        			ret "other";
        		},
        	};
        }

        fun main() {
        	print(classify(0));
        	print(classify(7));
        }
        "#,
        "zero\nother\n",
    );
}

// The async instance of the same function: the inferred-async pass runs
// over the same tail, and the false mismatch reproduced there too.
#[test]
fn an_async_function_whose_tail_is_an_if_else_of_rets_is_not_a_missing_return() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::time::sleep;

        fun classify(value: i32): str {
        	sleep(1);
        	if value > 0 {
        		ret "positive";
        	} else {
        		ret "non-positive";
        	}
        }

        fun main() {
        	print(classify(1));
        }
        "#,
        "positive\n",
    );
}

// A closure whose body is an exhaustive `if`/`else` of value-`ret`s now
// infers like a function (rule 4 lifted to the reachable-tail rule, B133):
// the dead tail is no evidence, the `ret`s agree on `str`, and the program
// runs. Re-pinned from
// `a_closure_of_rets_loses_the_false_mismatch_and_keeps_rule_4s_guidance`
// (which pinned the pre-lift conservative refusal): same program, the new
// rule.
#[test]
fn b133_a_closure_of_rets_infers_like_a_function() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun run(f: |i32| str): str {
        	f(1)
        }

        fun main() {
        	let out = run(|value| {
        		if value > 0 {
        			ret "positive";
        		} else {
        			ret "non-positive";
        		}
        	});
        	print(out);
        }
        "#,
        "positive\n",
    );
}

// The boundary that `tail_yields_no_value` guards: a closure whose body
// leaves by BARE `ret`s yields nothing, exactly as a void-tailed one does,
// so rule 4's bare-`ret` leg must stay silent rather than newly complain
// that the body "yields never".
#[test]
fn a_closure_whose_body_is_an_if_else_of_bare_rets_stays_legal() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun run(f: |i32|) {
        	f(1);
        }

        fun main() {
        	run(|value| {
        		if value > 0 {
        			ret;
        		} else {
        			ret;
        		}
        	});
        	print("done");
        }
        "#,
        "done\n",
    );
}

// --- B124's negatives: the missing-return diagnostics the fix must NOT
// weaken. `expr_diverges` needs EVERY path out to leave, so anything that
// can fall through is still diagnosed.

// No `else`, so the `if` falls through — regime 2's wording, unchanged.
#[test]
fn an_if_with_no_else_of_rets_still_reports_the_missing_return() {
    assert_fails_with(
        r#"
        fun classify(value: i32): str {
        	if value > 0 {
        		ret "positive";
        	}
        }

        fun main() {
        	classify(1);
        }
        "#,
        "Expected str, but got void instead: an `if` with no `else` produces void.",
    );
}

// One branch leaves, the other reaches its end without a value: the `if`
// still merges to void, and the missing return is still a real mistake.
#[test]
fn an_if_branch_that_rets_beside_one_that_falls_through_still_reports() {
    assert_fails_with(
        r#"
        fun classify(value: i32): str {
        	if value > 0 {
        		ret "positive";
        	} else {
        		let ignored = 1;
        	}
        }

        fun main() {
        	classify(1);
        }
        "#,
        "Expected str, but got void instead.",
    );
}

// A live branch whose value is the WRONG type, beside a leaving one: the
// leaving branch yields in the merge, so the mismatch is the live branch's
// own and is still reported against the declared type.
#[test]
fn a_wrongly_typed_branch_beside_a_ret_branch_is_still_diagnosed() {
    assert_fails_with(
        r#"
        fun classify(value: i32): str {
        	if value > 0 {
        		ret "positive";
        	} else {
        		5
        	}
        }

        fun main() {
        	classify(1);
        }
        "#,
        "Expected str, but got i32 instead.",
    );
}

// A `ret` whose VALUE is wrong inside an otherwise-exhaustive `if`/`else`:
// each `ret` still owns its own return-position constraint, which is the
// whole reason the unreachable tail after it needs no second check.
#[test]
fn a_mistyped_ret_inside_an_exhaustive_if_else_is_still_diagnosed() {
    assert_fails_with(
        r#"
        fun classify(value: i32): str {
        	if value > 0 {
        		ret 5;
        	} else {
        		ret "non-positive";
        	}
        }

        fun main() {
        	classify(1);
        }
        "#,
        "Expected str, but got i32 instead.",
    );
}

// A generic return type checks `ret` by unification, exactly like the tail.
#[test]
fn generic_return_rets_bind_like_the_tail() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::format;

        fun pick<T>(flag: bool, a: T, b: T): T {
        	if flag {
        		ret a;
        	}
        	b
        }

        fun main() {
        	print(format(pick(true, 1, 2)));
        	print(pick(false, "x", "y"));
        }
        "#,
        "1\ny\n",
    );
}

// `ret` is a first-class return position: a return-position generic call binds
// its type parameters from the declared type through `ret`, like the tail.
#[test]
fn ret_directs_return_position_generics() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::format;

        fun fresh(flag: bool): List<i32> {
        	if flag {
        		ret List::new();
        	}
        	[7]
        }

        fun main() {
        	print(format(fresh(true).len()));
        	print(format(fresh(false).len()));
        }
        "#,
        "0\n1\n",
    );
}

// An `async` function's `ret` checks against its declared return type.
#[test]
fn async_function_rets_check_against_the_declared_type() {
    assert_fails(
        r#"
        async fun bad(flag: bool): i32 {
        	if flag {
        		ret "nope";
        	}
        	1
        }

        async fun main() {
        	let _ = await bad(true);
        }
        "#,
    );
}

// `ret` returns from the NEAREST callable: a closure (or `async` block) is its
// own boundary — at runtime `ret` exits the closure, not the function, and an
// agreeing early-exit ret checks cleanly against the body's tail type.
#[test]
fn ret_inside_a_closure_exits_the_closure() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::format;

        fun apply(f: |i32| i32): i32 {
        	f(10)
        }

        fun main() {
        	let result = apply(|x| {
        		if x > 5 {
        			ret 99;
        		}
        		x + 1
        	});
        	print(format(result));
        	print("after");
        }
        "#,
        "99\nafter\n",
    );
}

// A closure's `ret` PARTICIPATES in its return typing: a ret disagreeing with
// the body's tail type is rejected (the collected-rets constraint —
// proposal/ret-checking.md rule 4's follow-up, now shipped).
#[test]
fn ret_participates_in_closure_return_inference() {
    assert_fails(
        r#"
        fun apply(f: |i32| i32): i32 {
        	f(10)
        }

        fun main() {
        	let _ = apply(|x| {
        		if x > 5 {
        			ret "mismatched";
        		}
        		x + 1
        	});
        }
        "#,
    );
}

// A trait-typed `self` returns through a trait-typed signature (the
// `impl Iterator<type T> with Iterable<T> { fun iter(self): Self { self } }`
// shape) — pins the `(Trait, Trait)` reconcile arm the return check surfaced.
//
// Spelled `Self`, as std's own `Iterable` is since B4's §11 migration: the
// declarations always meant "this type", and the trait's name in a return was
// only ever a stand-in for it. The arm stays load-bearing — the impl's subject
// IS a trait, so `self` and the declared return are both `Type::Trait` — which
// is why §11 keeps it priced rather than retiring it with the spelling.
#[test]
fn a_trait_typed_self_returns_through_a_trait_typed_signature() {
    assert_compiles(
        r#"
        import std::option::Option::{ self, Some, None };

        trait Walk<T> {
        	fun step(self): Option<T>;
        }

        trait AsWalk<T> {
        	fun as_walk(self): Self;
        }

        impl Walk<type T> with AsWalk<T> {
        	fun as_walk(self): Self {
        		self
        	}
        }

        fun main() {}
        "#,
    );
}

// --- Diagnostic span precision (backlog E7) ------------------------------------
// Each pins that the error's span covers exactly the PERTINENT expression, not
// an enclosing aggregate — a regression back to the coarse span fails the
// exact-range assertion.

// A match-leg mismatch points at the offending leg's body, not the whole match.
#[test]
fn match_leg_mismatch_spans_the_offending_leg() {
    assert_fails_spanning(
        r#"
        fun pick(flag: bool): i32 {
        	match flag {
        		true => 1,
        		false => "oops",
        	}
        }

        fun main() {
        	let _ = pick(true);
        }
        "#,
        "\"oops\"",
        "match legs have mismatched types",
    );
}

// A struct-initializer field mismatch points at that field's value, not the
// whole `{ .. }` block.
#[test]
fn struct_field_mismatch_spans_the_field_value() {
    assert_fails_spanning(
        r#"
        struct Point {
        	x: i32,
        	y: i32,
        }

        fun main() {
        	let _ = Point { x = 1, y = "two" };
        }
        "#,
        "\"two\"",
        "Expected i32, but got str",
    );
}

// An unknown struct name anchors at the initializer (which includes the name),
// not the field block alone.
#[test]
fn unknown_struct_spans_the_initializer() {
    assert_fails_spanning(
        r#"
        fun main() {
        	let _ = Pointt { x = 1 };
        }
        "#,
        "Pointt { x = 1 }",
        "unknown struct",
    );
}

// --- E58: closest-name suggestion on an invalid initializer field --------------
// The scan (vilan_core::closest_name) runs ONLY once a field name has already
// failed to resolve — the calls below all live inside the "no such field" arm,
// so a struct initializer that resolves cleanly never reaches it at all.

// An unknown initializer field now anchors on the NAME, not the value it was
// given — the span the E58 rename quickfix rewrites (main.rs/document.rs).
// This also proves the count-mismatch shape untouched by the same edit: it
// returns before this per-field loop even runs.
#[test]
fn unknown_initializer_field_spans_the_name_not_the_value() {
    assert_fails_spanning(
        r#"
        struct Config {
        	entries: i32,
        }

        fun main() {
        	let _ = Config { entires = 5 };
        }
        "#,
        "entires",
        "struct 'Config' has no field 'entires'",
    );
}

// S5 (editing-dx.md §7.2, P20): the survey's exact reproduction of the OLD
// bug — widen the VALUE and confirm the underline does NOT widen with it.
// Before E58 the span tracked `field_value_span` unconditionally, so a
// five-character value produced a five-character underline three columns
// away from the name it was supposedly about; P19's `= 5` alone doesn't
// distinguish "anchored on the name" from "anchored on a value that happens
// to be short", since both are one character. This is what proves the
// anchor MOVED, not just that it currently sits somewhere plausible.
#[test]
fn unknown_initializer_field_with_a_wide_value_still_spans_the_name() {
    assert_fails_spanning(
        r#"
        struct Point {
        	x: i32,
        	y: i32,
        }

        fun main() {
        	let _ = Point { x = 3, yy = 40000 };
        }
        "#,
        "yy",
        "struct 'Point' has no field 'yy'",
    );
}

// A clear typo of a real field gets a "did you mean" note, anchored at the
// misspelled name — the threshold's suggest side (a single transposed pair).
#[test]
fn unknown_initializer_field_notes_a_close_typo() {
    assert_fails_noting(
        r#"
        struct Config {
        	entries: i32,
        }

        fun main() {
        	let _ = Config { entires = 5 };
        }
        "#,
        "struct 'Config' has no field 'entires'",
        "entires",
        "did you mean `entries`?",
    );
}

// The threshold's refuse side: a name with almost nothing in common with the
// real field gets no note at all — the diagnostic still fires, bare.
#[test]
fn unknown_initializer_field_far_from_every_real_field_gets_no_note() {
    let source = r#"
        struct Config {
        	entries: i32,
        }

        fun main() {
        	let _ = Config { x = 5 };
        }
        "#;
    let diagnostics = failure_diagnostics_with_notes(source);
    let matching: Vec<_> = diagnostics
        .iter()
        .filter(|(message, _, _)| message.contains("struct 'Config' has no field 'x'"))
        .collect();
    assert_eq!(matching.len(), 1, "got: {diagnostics:#?}");
    assert!(
        matching[0].2.is_none(),
        "expected no note on a far-off field name; got: {:#?}",
        matching[0]
    );
}

// Structural non-vacuity for the "runs only on the invalid-name path" rule
// (CLAUDE.md: "assert the diagnostic path, not performance" — the scan call
// sits textually inside the `None` arm of the field lookup, so a program that
// never reaches that arm never reaches the scan): a correctly-named
// initializer produces no diagnostic at all, note included.
#[test]
fn a_correctly_named_initializer_field_gets_no_diagnostic() {
    assert_compiles(
        r#"
        struct Config {
        	entries: i32,
        }

        fun main() {
        	let _ = Config { entries = 5 };
        }
        "#,
    );
}

// The field-COUNT mismatch is a different diagnostic entirely (it returns
// before the per-field, closest-name-scanning loop even runs) — pinned
// unaffected by the E58 edit. Message per S4 (editing-dx.md §7.1): names
// the struct and the missing field.
#[test]
fn initializer_field_count_mismatch_is_unaffected_by_the_closest_name_scan() {
    assert_fails_with(
        r#"
        struct Config {
        	entries: i32,
        	limit: i32,
        }

        fun main() {
        	let _ = Config { entries = 5 };
        }
        "#,
        "`Config` expects 2 fields, but got 1 instead: `limit` is missing.",
    );
}

// A missing import segment points at that segment, not the whole statement.
#[test]
fn import_segment_error_spans_the_segment() {
    assert_fails_spanning(
        r#"
        import std::option::Optionn;

        fun main() {}
        "#,
        "Optionn",
        "cannot find 'Optionn' in the imported path",
    );
}

// An unknown import ROOT points at the root segment.
#[test]
fn import_root_error_spans_the_root() {
    assert_fails_spanning(
        r#"
        import nowhere::thing;

        fun main() {}
        "#,
        "nowhere",
        "cannot find module 'nowhere' to import",
    );
}

// A missing `use` segment points at that segment.
#[test]
fn use_segment_error_spans_the_segment() {
    assert_fails_spanning(
        r#"
        import std::option::Option;

        fun main() {
        	use Option::Somme;
        	let _ = 1;
        }
        "#,
        "Somme",
        "cannot find 'Somme' in the `use` path",
    );
}

// --- `expr!` — assert-or-return (proposal/try-and-lift.md, slice 1) -------------

// The happy and early paths, on both std types, with the early return proven
// by an unreached side effect.
#[test]
fn bang_unwraps_good_and_returns_bad() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::format;
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };

        fun lookup(key: str): Option<i32> {
        	if key == "hit" {
        		Some(21)
        	} else {
        		None
        	}
        }

        fun doubled(key: str): Option<i32> {
        	let value = lookup(key)!;
        	print("unwrapped");
        	Some(value * 2)
        }

        fun to_number(text: str): Result<i32, str> {
        	match text.parse_i32() {
        		Some(let value) => Ok(value),
        		None => Err(i"not a number: {text}"),
        	}
        }

        fun sum(a: str, b: str): Result<i32, str> {
        	let left = to_number(a)!;
        	let right = to_number(b)!;
        	Ok(left + right)
        }

        fun main() {
        	match doubled("hit") {
        		Some(let v) => print(i"some {format(v)}"),
        		None => print("none"),
        	}
        	match doubled("miss") {
        		Some(let v) => print(i"some {format(v)}"),
        		None => print("none"),
        	}
        	match sum("2", "40") {
        		Ok(let v) => print(i"ok {format(v)}"),
        		Err(let e) => print(i"err {e}"),
        	}
        	match sum("2", "forty") {
        		Ok(let v) => print(i"ok {format(v)}"),
        		Err(let e) => print(i"err {e}"),
        	}
        }
        "#,
        "unwrapped\nsome 42\nnone\nok 42\nerr not a number: forty\n",
    );
}

// A user `Try` type behaves exactly like the std pair — the §8.3 equivalence
// pin: real trait dispatch through `verdict`/`from_bad`.
#[test]
fn a_user_try_type_behaves_like_the_std_pair() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::format;
        import std::operators::{ Try, Verdict };

        enum Lint {
        	Clean(i32),
        	Dirty(str),
        }

        impl Lint with Try<i32, str> {
        	fun verdict(self): Verdict<i32, str> {
        		match self {
        			Lint::Clean(let score) => Verdict::Good(score),
        			Lint::Dirty(let complaint) => Verdict::Bad(complaint),
        		}
        	}

        	fun from_bad(bad: str): Lint {
        		Lint::Dirty(bad)
        	}
        }

        fun check(source: str): Lint {
        	if source == "tidy" {
        		Lint::Clean(95)
        	} else {
        		Lint::Dirty(i"messy: {source}")
        	}
        }

        fun grade(source: str): Lint {
        	let score = check(source)!;
        	print("scored");
        	Lint::Clean(score + 5)
        }

        fun main() {
        	match grade("tidy") {
        		Lint::Clean(let score) => print(i"clean {format(score)}"),
        		Lint::Dirty(let complaint) => print(complaint),
        	}
        	match grade("sloppy") {
        		Lint::Clean(let score) => print(i"clean {format(score)}"),
        		Lint::Dirty(let complaint) => print(complaint),
        	}
        }
        "#,
        "scored\nclean 100\nmessy: sloppy\n",
    );
}

// `!` works in async functions (the declared return type is the frame).
#[test]
fn bang_works_in_async_functions() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::format;
        import std::result::Result::{ self, Ok, Err };

        async fun fetch_number(flag: bool): Result<i32, str> {
        	if flag {
        		Ok(7)
        	} else {
        		Err("offline")
        	}
        }

        async fun doubled(flag: bool): Result<i32, str> {
        	let value = (await fetch_number(flag))!;
        	Ok(value * 2)
        }

        async fun main() {
        	match await doubled(true) {
        		Ok(let v) => print(i"ok {format(v)}"),
        		Err(let e) => print(e),
        	}
        	match await doubled(false) {
        		Ok(let v) => print(i"ok {format(v)}"),
        		Err(let e) => print(e),
        	}
        }
        "#,
        "ok 14\noffline\n",
    );
}

// `!` binds tighter than comparison, and `a!=b` stays a comparison (the lex
// rule: `!=` wins; the postfix form needs the space).
#[test]
fn bang_spacing_against_not_equals() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        fun pick(): Option<i32> {
        	Some(3)
        }

        fun compare(): Option<bool> {
        	let a = 3;
        	let b = 4;
        	// `a!=b` is not-equals on plain values...
        	if a!=b {
        		print("a != b");
        	}
        	// ...while `pick()! == a` unwraps then compares.
        	Some(pick()! == a)
        }

        fun main() {
        	match compare() {
        		Some(let equal) => print(if equal { "equal" } else { "not equal" }),
        		None => print("none"),
        	}
        }
        "#,
        "a != b\nequal\n",
    );
}

// The error cases, each pinned at the pertinent span (E7 harness).
#[test]
fn bang_on_option_requires_an_option_function() {
    assert_fails_spanning(
        r#"
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };

        fun lookup(): Option<i32> {
        	Some(1)
        }

        fun bad(): Result<i32, str> {
        	let value = lookup()!;
        	Ok(value)
        }

        fun main() {
        	let _ = bad();
        }
        "#,
        "lookup()!",
        ".ok_or(err)",
    );
}

#[test]
fn bang_result_error_types_must_match() {
    assert_fails_spanning(
        r#"
        import std::result::Result::{ self, Ok, Err };

        fun inner(): Result<i32, str> {
        	Ok(1)
        }

        fun bad(): Result<i32, i32> {
        	let value = inner()!;
        	Ok(value)
        }

        fun main() {
        	let _ = bad();
        }
        "#,
        "inner()!",
        "Convert the error first: `.map_err(…)`",
    );
}

#[test]
fn explicit_error_conversion_composes_with_bang() {
    // `!` stays same-type (no implicit `From`/`Into` — the no-silent-conversion
    // rule); crossing error types is EXPLICIT at the value, before the `!`. The
    // std combinators compose: `.map_err(f)!` maps `E1 → E2` (a named fn or a
    // closure), and `.ok_or(err)!` turns an `Option`'s `None` into a supplied
    // `Err`. All three run and the converted error reaches the caller.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::result::Result::{ self, Ok, Err };
        import std::option::Option::{ self, Some, None };

        struct DbError { code: i32 }
        struct AppError { msg: str }
        fun to_app(e: DbError): AppError { AppError { msg = "db" } }

        fun query(): Result<i32, DbError> { Err(DbError { code = 7 }) }
        fun parse(text: str): Result<i32, str> { Err(text) }
        fun find(): Option<i32> { None }

        fun via_named(): Result<i32, AppError> {
            let value = query().map_err(to_app)!;      // E1 -> E2, named fn
            Ok(value)
        }
        fun via_closure(): Result<i32, AppError> {
            let value = parse("oops").map_err(|e| AppError { msg = e })!;  // closure
            Ok(value)
        }
        fun via_ok_or(): Result<i32, AppError> {
            let value = find().ok_or(AppError { msg = "missing" })!;  // Option -> Result
            Ok(value)
        }

        fun show(result: Result<i32, AppError>) {
            match result {
                Ok(let v) => { print(v); },
                Err(let e) => { print(e.msg); },
            }
        }
        fun main() {
            show(via_named());     // db
            show(via_closure());   // oops
            show(via_ok_or());     // missing
        }
        "#,
        "db\noops\nmissing\n",
    );
}

#[test]
fn bang_in_a_bare_void_function_is_rejected() {
    assert_fails_spanning(
        r#"
        import std::option::Option::{ self, Some, None };

        fun lookup(): Option<i32> {
        	Some(1)
        }

        fun bad() {
        	let _ = lookup()!;
        }

        fun main() {
        	bad();
        }
        "#,
        "lookup()!",
        "requires the nearest enclosing function",
    );
}

#[test]
fn bang_in_a_closure_is_rejected_v1() {
    assert_fails_spanning(
        r#"
        import std::option::Option::{ self, Some, None };

        fun lookup(): Option<i32> {
        	Some(1)
        }

        fun outer(): Option<i32> {
        	let helper = |x: i32| {
        		let value = lookup()!;
        		value + x
        	};
        	Some(helper(1))
        }

        fun main() {
        	let _ = outer();
        }
        "#,
        "lookup()!",
        "closures and `async` blocks are not yet supported",
    );
}

#[test]
fn bang_on_a_non_try_type_is_rejected() {
    assert_fails_spanning(
        r#"
        import std::option::Option::{ self, Some, None };

        fun bad(): Option<i32> {
        	let n = 5;
        	let value = n!;
        	Some(value)
        }

        fun main() {
        	let _ = bad();
        }
        "#,
        "n!",
        "needs a value implementing `Try`",
    );
}

// A user `Try` type's enclosing return must equal the receiver exactly (v1).
#[test]
fn user_try_requires_the_exact_return_type() {
    assert_fails_spanning(
        r#"
        import std::option::Option::{ self, Some, None };
        import std::operators::{ Try, Verdict };

        enum Lint {
        	Clean(i32),
        	Dirty(str),
        }

        impl Lint with Try<i32, str> {
        	fun verdict(self): Verdict<i32, str> {
        		match self {
        			Lint::Clean(let score) => Verdict::Good(score),
        			Lint::Dirty(let complaint) => Verdict::Bad(complaint),
        		}
        	}

        	fun from_bad(bad: str): Lint {
        		Lint::Dirty(bad)
        	}
        }

        fun check(): Lint {
        	Lint::Clean(1)
        }

        fun bad(): Option<i32> {
        	let score = check()!;
        	Some(score)
        }

        fun main() {
        	let _ = bad();
        }
        "#,
        "check()!",
        "must match exactly",
    );
}

// `void` is the unit expression — the unit type's one value, usable wherever a
// void-typed value is (generic arguments included).
#[test]
fn void_is_the_unit_expression() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };

        fun consume(value: void): i32 {
        	7
        }

        fun confirm(flag: bool): Result<void, str> {
        	if flag {
        		Ok(void)
        	} else {
        		Err("refused")
        	}
        }

        fun main() {
        	print(consume(void));
        	let unit: Option<void> = Some(void);
        	match unit {
        		Some(let _v) => print("some unit"),
        		None => print("none"),
        	}
        	match confirm(true) {
        		Ok(let _v) => print("confirmed"),
        		Err(let e) => print(e),
        	}
        }
        "#,
        "7\nsome unit\nconfirmed\n",
    );
}

// --- `a?.b` — lifted member chains (proposal/try-and-lift.md, slice 2) ----------

// Map and flatten, typed and run: a plain-valued continuation wraps back into
// the container; a container-valued one flattens (single Option, not nested).
// The None subject short-circuits — proven by an unreached side effect.
#[test]
fn lift_maps_flattens_and_short_circuits() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::format;
        import std::option::Option::{ self, Some, None };

        struct Profile {
        	name: str,
        }

        impl Profile {
        	fun loud_name(self): str {
        		print("computed");
        		self.name
        	}

        	fun nickname(self): Option<str> {
        		if self.name == "ada" {
        			Some("the countess")
        		} else {
        			None
        		}
        	}
        }

        fun user(key: str): Option<Profile> {
        	if key == "hit" {
        		Some(Profile { name = "ada" })
        	} else {
        		None
        	}
        }

        fun main() {
        	// map — the annotation pins the type: Option<str>, not nested.
        	let mapped: Option<str> = user("hit")?.loud_name();
        	print(mapped.unwrap_or("?"));
        	// short-circuit: the continuation must not run.
        	let skipped: Option<str> = user("miss")?.loud_name();
        	print(skipped.unwrap_or("?"));
        	// flatten — the annotation pins Option<str> (not Option<Option<str>>).
        	let flat: Option<str> = user("hit")?.nickname();
        	print(flat.unwrap_or("?"));
        	let flat_none: Option<str> = user("miss")?.nickname();
        	print(flat_none.unwrap_or("?"));
        	// multi-link with args, escaped by parens.
        	print(format((user("hit")?.nickname()?.len()).unwrap_or(0 - 1)));
        }
        "#,
        "computed\nada\n?\nthe countess\n?\n12\n",
    );
}

// Result lifts: map wraps Ok, flatten passes the chain's own Result through,
// and Err short-circuits as-is.
#[test]
fn lift_works_on_results() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::format;
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };

        fun to_number(text: str): Result<i32, str> {
        	match text.parse_i32() {
        		Some(let value) => Ok(value),
        		None => Err(i"bad: {text}"),
        	}
        }

        fun halve(value: i32): Result<i32, str> {
        	if value == value / 2 * 2 {
        		Ok(value / 2)
        	} else {
        		Err("odd")
        	}
        }

        fun show(value: Result<i32, str>) {
        	match value {
        		Ok(let v) => print(i"ok {format(v)}"),
        		Err(let e) => print(e),
        	}
        }

        fun main() {
        	let mapped: Result<i32, str> = to_number("21")?.max(0);
        	show(mapped);
        	let flat: Result<i32, str> = to_number("42")?.abs()?.max(0);
        	show(flat);
        	show(to_number("nope")?.max(0));
        }
        "#,
        "ok 21\nok 42\nbad: nope\n",
    );
}

// `?.` composes with `!`: the bang applies to the LIFTED result (it closes the
// group), not inside the continuation.
#[test]
fn lift_composes_with_bang() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        struct Wrap {
        	label: str,
        }

        fun boxed(key: str): Option<Wrap> {
        	if key == "hit" {
        		Some(Wrap { label = "inside" })
        	} else {
        		None
        	}
        }

        fun read(key: str): Option<str> {
        	let label = boxed(key)?.label!;
        	Some(label)
        }

        fun main() {
        	match read("hit") {
        		Some(let v) => print(v),
        		None => print("none"),
        	}
        	match read("miss") {
        		Some(let v) => print(v),
        		None => print("none"),
        	}
        }
        "#,
        "inside\nnone\n",
    );
}

// `?.` on a non-Lift subject is rejected at the chain's span.
#[test]
fn lift_on_a_non_lift_type_is_rejected() {
    assert_fails_spanning(
        r#"
        fun main() {
        	let n = 5;
        	let _ = n?.max(1);
        }
        "#,
        "n?.max(1)",
        "`?.` lifts an `Option`, a `Result`, or a type opting in",
    );
}

// A flattened Result chain must keep the same error type.
#[test]
fn lift_flatten_requires_matching_result_errors() {
    assert_fails_spanning(
        r#"
        import std::result::Result::{ self, Ok, Err };

        fun start(): Result<i32, str> {
        	Ok(1)
        }

        struct Helper {}

        impl i32 {
        	fun widen(self): Result<i32, i32> {
        		Ok(self)
        	}
        }

        fun main() {
        	let _ = start()?.widen();
        }
        "#,
        "start()?.widen()",
        "Convert the error first with `.map_err(…)`",
    );
}

// A bare `?` (no following member) does not parse.
#[test]
fn bare_question_mark_is_rejected() {
    assert_fails(
        r#"
        import std::option::Option::{ self, Some, None };

        fun main() {
        	let a = Some(1);
        	let _ = a?;
        }
        "#,
    );
}

// A lifted chain is not an assignment target.
#[test]
fn lift_is_not_an_assignment_target() {
    assert_fails(
        r#"
        import std::option::Option::{ self, Some, None };

        struct Point {
        	x: i32,
        }

        fun main() {
        	let p = Some(Point { x = 1 });
        	p?.x = 5;
        }
        "#,
    );
}

// A RETURN-position generic binds THROUGH `!`: the let's annotation directs
// the receiver's type parameter (`resolve_try_assert` re-infers the receiver
// as `Container<expected, ..>` once the container is known, riding the same
// reconcile-and-record channel as an annotated let).
#[test]
fn bang_directs_return_position_generics_into_its_receiver() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::format;
        import std::result::Result::{ self, Ok, Err };
        import std::json::FromJson;

        fun decode_as<T: FromJson>(text: str): Result<T, str> {
        	T::from_json(text)
        }

        fun run(): Result<i32, str> {
        	let n: i32 = decode_as("42")!;
        	Ok(n)
        }

        fun main() {
        	match run() {
        		Ok(let v) => print(format(v)),
        		Err(let e) => print(e),
        	}
        }
        "#,
        "42\n",
    );
}

// The bare-`ret` half of closure participation: fine in a void-tailed closure,
// rejected in a value-yielding one...
#[test]
fn bare_ret_in_a_value_yielding_closure_is_rejected() {
    assert_fails_spanning(
        r#"
        fun apply(f: |i32| i32): i32 {
        	f(10)
        }

        fun main() {
        	let _ = apply(|x| {
        		if x > 5 {
        			ret;
        		}
        		x + 1
        	});
        }
        "#,
        "ret",
        "a bare `ret` exits a closure whose body yields",
    );
}

// ...and the mirror: a value-`ret` in a closure whose body ends without one.
#[test]
fn value_ret_in_a_void_closure_is_rejected() {
    assert_fails_spanning(
        r#"
        import std::io::print;

        fun main() {
        	let helper = |x: i32| {
        		if x > 5 {
        			ret 99;
        		}
        		print("small");
        	};
        	helper(1);
        }
        "#,
        "ret 99",
        "make the ret'd value the body's tail",
    );
}

// A bare-ret early exit in a void closure stays legal (the guard pattern).
#[test]
fn bare_ret_in_a_void_closure_is_allowed() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
        	let helper = |x: i32| {
        		if x > 5 {
        			ret;
        		}
        		print("small");
        	};
        	helper(10);
        	helper(1);
        }
        "#,
        "small\n",
    );
}

// `async` blocks get the same participation: an agreeing ret passes, and the
// existing early-return semantics hold.
#[test]
fn async_block_rets_check_against_the_tail() {
    assert_fails_spanning(
        r#"
        fun main() {
        	let flag = true;
        	let pending = async {
        		if flag {
        			ret "mismatched";
        		}
        		2
        	};
        }
        "#,
        "ret \"mismatched\"",
        "but the closure's body yields",
    );
}

// A user `Lift` container: `?.` dispatches to ITS `map`/`and_then` (the tag
// concatenation proves the user's and_then body ran on the flatten path).
#[test]
fn a_user_lift_container_dispatches_to_its_own_map_and_and_then() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::display::format;
        import std::operators::Lift;

        struct Boxy<T> {
        	value: T,
        	tag: str,
        }

        impl Boxy<type T> with Lift {}

        impl Boxy<type T> {
        	fun map<U>(self, fn: |T| U): Boxy<U> {
        		Boxy { value = fn(self.value), tag = self.tag }
        	}

        	fun and_then<U>(self, fn: |T| Boxy<U>): Boxy<U> {
        		let inner = fn(self.value);
        		Boxy { value = inner.value, tag = self.tag + "+" + inner.tag }
        	}
        }

        struct Profile {
        	name: str,
        }

        impl Profile {
        	fun boxed_name(self): Boxy<str> {
        		Boxy { value = self.name, tag = "inner" }
        	}
        }

        fun main() {
        	let boxed = Boxy { value = Profile { name = "ada" }, tag = "outer" };
        	let mapped: Boxy<str> = boxed?.name;
        	print(i"{mapped.value} [{mapped.tag}]");
        	let lengths: Boxy<i32> = boxed?.name.len();
        	print(format(lengths.value));
        	let flat: Boxy<str> = boxed?.boxed_name();
        	print(i"{flat.value} [{flat.tag}]");
        }
        "#,
        "ada [outer]\n3\nada [outer+inner]\n",
    );
}

// The marker is the gate: a mappable type WITHOUT `impl .. with Lift` refuses.
#[test]
fn a_mappable_type_without_the_lift_marker_is_rejected() {
    assert_fails_spanning(
        r#"
        struct Sneaky<T> {
        	value: T,
        }

        impl Sneaky<type T> {
        	fun map<U>(self, fn: |T| U): Sneaky<U> {
        		Sneaky { value = fn(self.value) }
        	}
        }

        fun main() {
        	let s = Sneaky { value = 1 };
        	let _ = s?.max(2);
        }
        "#,
        "s?.max(2)",
        "opting in with `impl .. with Lift`",
    );
}

// --- Expression lifting `a? + 10` / `a? + b?` (proposal/expression-lifting.md) ---

#[test]
fn expression_lift_maps_a_single_receiver() {
    // One bare `?`: the rest of the expression is the continuation; the
    // region types as the container of the body (`Option<i32>` here).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            let count = Some(2);
            let doubled: Option<i32> = count? * 2;
            print(doubled.unwrap_or(-1));   // 4
            let missing: Option<i32> = None;
            print((missing? * 2).unwrap_or(-1));   // -1 — None short-circuits
        }
        "#,
        "4\n-1\n",
    );
}

#[test]
fn expression_lift_operands_are_symmetrical() {
    // The `?` may mark either operand — and a call LEFT of a bad `?` still
    // runs (source evaluation order; the hoisted eval step).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        fun bump(log: &mut List<i32>): i32 {
            log.push(1);
            10
        }
        fun main() {
            let count = Some(4);
            print((2 * count?).unwrap_or(-1));   // 8
            mut log: List<i32> = [];
            let missing: Option<i32> = None;
            let compared: Option<bool> = bump(&mut log) < missing?;
            print(compared.is_some());   // false — the region is None…
            print(log.len());            // 1 — …but bump ran (left of the ?)
        }
        "#,
        "8\nfalse\n1\n",
    );
}

#[test]
fn expression_lift_applicative_short_circuits_lazily() {
    // Two `?`s: good only if both are; a receiver RIGHT of a bad `?` is not
    // evaluated (the `&&` precedent) — pinned through the log.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        fun fetch(log: &mut List<i32>, value: Option<i32>): Option<i32> {
            log.push(1);
            value
        }
        fun main() {
            mut log: List<i32> = [];
            let total = fetch(&mut log, Some(40))? + fetch(&mut log, Some(2))?;
            print(total.unwrap_or(-1));   // 42
            print(log.len());             // 2 — both ran
            mut log2: List<i32> = [];
            let bad = fetch(&mut log2, None)? + fetch(&mut log2, Some(2))?;
            print(bad.unwrap_or(-1));     // -1
            print(log2.len());            // 1 — the right receiver never ran
        }
        "#,
        "42\n2\n-1\n1\n",
    );
}

#[test]
fn expression_lift_on_results_first_error_wins() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::result::Result::{ self, Ok, Err };
        fun parse(tag: str): Result<i32, str> {
            if tag == "good" { Ok(21) } else { Err("bad: " + tag) }
        }
        fun main() {
            let sum = parse("good")? + parse("good")?;
            match sum {
                Ok(let n) => print(n),          // 42
                Err(let e) => print(e),
            }
            let first = parse("x")? + parse("y")?;
            match first {
                Ok(let n) => print(n),
                Err(let e) => print(e),          // bad: x — the FIRST error
            }
        }
        "#,
        "42\nbad: x\n",
    );
}

#[test]
fn expression_lift_result_receivers_need_one_error_type() {
    // One region has one result type, so two `Result` receivers must carry
    // the same `E` (§6.5's corollary) — with the explicit-conversion hint.
    assert_fails_with(
        r#"
        import std::result::Result::{ self, Ok, Err };
        struct Wrapped { msg: str }
        fun a(): Result<i32, str> { Ok(1) }
        fun b(): Result<i32, Wrapped> { Ok(2) }
        fun main() {
            let sum = a()? + b()?;
        }
        "#,
        "Convert the error first with `.map_err(…)`",
    );
}

#[test]
fn expression_lift_mixed_containers_are_rejected() {
    assert_fails_with(
        r#"
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };
        fun main() {
            let opt = Some(1);
            let res: Result<i32, str> = Ok(2);
            let sum = opt? + res?;
        }
        "#,
        "must split the same container",
    );
}

#[test]
fn expression_lift_flattens_a_container_body() {
    // The body yields the receivers' own container (`rows?[0]` on an
    // `Option<List<Option<i32>>>`) — one level, not `Option<Option<_>>`
    // (the chain rule, inherited; pinned by the annotation).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            let rows: Option<List<Option<i32>>> = Some([Some(7), None]);
            let first: Option<i32> = rows?[0];
            print(first.unwrap_or(-1));   // 7
        }
        "#,
        "7\n",
    );
}

#[test]
fn expression_lift_identity_is_rejected() {
    // A region whose body is just the hole computes nothing — a hard error
    // (§6.3): `let x = a?;` and the argument-slot form `f(a?)` alike.
    assert_fails_with(
        r#"
        import std::option::Option::{ self, Some, None };
        fun main() {
            let a = Some(1);
            let x = a?;
        }
        "#,
        "`?` lifts nothing here",
    );
    assert_fails_with(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        fun describe(value: Option<i32>): str { "x" }
        fun main() {
            let a = Some(1);
            print(describe(a?));
        }
        "#,
        "`?` lifts nothing here",
    );
}

#[test]
fn expression_lift_in_a_condition_is_rejected() {
    // A condition is its own slot: the region lifts the comparison to
    // `Option<bool>`, which a condition cannot take — an EXPLICIT check
    // (conditions are not generally type-checked yet, and an Option is a
    // tagged array, i.e. always truthy — this would silently take the
    // branch), with the match steer.
    assert_fails_with(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            let a = Some(1);
            if a? > 0 {
                print("positive");
            }
        }
        "#,
        "which a condition cannot take",
    );
}

#[test]
fn expression_lift_never_absorbs_a_chain() {
    // `a?.b == None` keeps its shipped, container-typed meaning (§5 — the
    // absorption rejection): the chain is a sealed atom inside the region.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        struct User { name: str }
        fun main() {
            let user = Some(User { name = "ada" });
            print(user?.name == None);            // false — Option == Option
            let nobody: Option<User> = None;
            print(nobody?.name == None);          // true
        }
        "#,
        "false\ntrue\n",
    );
}

#[test]
fn expression_lift_parens_delimit_the_region() {
    // `(a? + 1)` seals at the paren and composes outside it; a lifted chain
    // in parens stays container-typed, so `(a?.b) + 1` is the ordinary
    // type error.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            let a = Some(41);
            let x: Option<i32> = (a? + 1);
            print(x.unwrap_or(-1));   // 42
        }
        "#,
        "42\n",
    );
    assert_fails(
        r#"
        import std::option::Option::{ self, Some, None };
        struct User { age: i32 }
        fun main() {
            let user = Some(User { age = 1 });
            let x = (user?.age) + 1;
        }
        "#,
    );
}

#[test]
fn expression_lift_rejects_bang_after_a_split() {
    // `!` may not run after a `?` in one region — it would early-return
    // from inside the lift.
    assert_fails_with(
        r#"
        import std::option::Option::{ self, Some, None };
        fun main(): Option<i32> {
            let a = Some(1);
            let b = Some(2);
            let x = a? + b!;
            None
        }
        "#,
        "`!` cannot run after a `?` inside a lifted expression",
    );
}

#[test]
fn expression_lift_composes_with_bang_outside() {
    // `(region)!` asserts on the lifted result — the region seals at the
    // paren, `!` applies to the whole `Option`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        fun total(a: Option<i32>, b: Option<i32>): Option<i32> {
            let sum = (a? + b?)!;
            Some(sum * 10)
        }
        fun main() {
            print(total(Some(4), Some(2)).unwrap_or(-1));   // 60
            print(total(Some(4), None).unwrap_or(-1));      // -1 — the ! returned
        }
        "#,
        "60\n-1\n",
    );
}

#[test]
fn expression_lift_twice_evaluated_receiver_is_legal() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            let size = Some(4);
            let area: Option<i32> = size? * size?;
            print(area.unwrap_or(-1));   // 16
        }
        "#,
        "16\n",
    );
}

#[test]
fn expression_lift_match_subject_region_works() {
    // A match subject is a slot, and a region there is meaningful: the legs
    // match the LIFTED value (`Option<i32>` here) — unlike a condition,
    // nothing needs `bool`, so it stays legal.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            let count = Some(2);
            match count? * 2 {
                Some(let n) => print(n),   // 4
                None => print("none"),
            }
            let missing: Option<i32> = None;
            match missing? * 2 {
                Some(let n) => print(n),
                None => print("none"),     // none
            }
        }
        "#,
        "4\nnone\n",
    );
}

#[test]
fn expression_lift_bare_iterable_is_the_identity_error() {
    // `for x in items?` — the iterable slot's region is just the hole, so
    // the identity-lift error fires: an Option isn't iterable; unwrap or
    // match first.
    assert_fails_with(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        fun main() {
            let items = Some([1, 2]);
            for x in items? {
                print(x);
            }
        }
        "#,
        "`?` lifts nothing here",
    );
}

// --- B163: an `if`'s arms are unified, by the rule `match`'s legs go through ---
// Before the fix the arms were never checked against each other: the `if` took
// its type from the FIRST arm and the others' values flowed out unchecked, so
// `let mixed = if c { 1 } else { "two" }` typed as `i32` and `mixed + 1`
// printed `two1`. `match` legs were already unified; the two constructs now
// share `unify_arm_bodies`, so the rule is stated once.

// The item's own shape: an unannotated `let` over a two-armed `if`.
#[test]
fn if_arms_of_different_types_are_refused_in_a_let() {
    assert_fails_with(
        r#"

        fun main() {
        	let c = false;
        	let mixed = if c { 1 } else { "two" };
        	print(mixed);
        }
        "#,
        "`if` arms have mismatched types: expected i32, but got str instead.",
    );
}

// The same mismatch through a declared return type: the `if` is the tail, and
// the wrong-typed arm used to be returned as the declared type.
#[test]
fn if_arms_of_different_types_are_refused_in_a_function_tail() {
    assert_fails_with(
        r#"
        fun pick(c: bool): i32 {
        	if c { 1 } else { "two" }
        }

        fun main() {
        	let _ = pick(false);
        }
        "#,
        "`if` arms have mismatched types",
    );
}

// An `else if` chain is one construct with three arms, not a nested pair: the
// mismatch in the MIDDLE arm is caught, and so is one in the final `else`.
#[test]
fn a_nested_else_if_chain_unifies_every_arm() {
    assert_fails_with(
        r#"
        fun pick(a: bool, b: bool): i32 {
        	if a { 1 } else if b { "two" } else { 3 }
        }

        fun main() {
        	let _ = pick(true, false);
        }
        "#,
        "`if` arms have mismatched types: expected i32, but got str instead.",
    );
}

#[test]
fn a_nested_else_if_chain_catches_a_mismatch_in_its_final_else() {
    assert_fails_with(
        r#"
        fun pick(a: bool, b: bool): i32 {
        	if a { 1 } else if b { 2 } else { "three" }
        }

        fun main() {
        	let _ = pick(true, false);
        }
        "#,
        "`if` arms have mismatched types: expected i32, but got str instead.",
    );
}

// The mismatch is anchored at the OFFENDING arm's tail, not at the whole `if`
// (E7 — the pertinent expression), exactly as a match leg's is.
#[test]
fn if_arm_mismatch_spans_the_offending_arm() {
    assert_fails_spanning(
        r#"
        fun pick(c: bool): i32 {
        	if c { 1 } else { "oops" }
        }

        fun main() {
        	let _ = pick(true);
        }
        "#,
        "\"oops\"",
        "`if` arms have mismatched types",
    );
}

// The green side of the rule: arms that genuinely unify still compile and run.
// A literal arm and a call arm agreeing on `i32`, and a nullable-shaped pair
// agreeing through an annotation.
#[test]
fn if_arms_that_unify_still_compile() {
    assert_compiles_and_runs(
        r#"

        fun double(value: i32): i32 { value * 2 }

        fun main() {
        	let c = false;
        	let picked = if c { 1 } else { double(4) };
        	print(picked);
        	let word: str = if c { "yes" } else { "no" };
        	print(word);
        }
        "#,
        "8\nno\n",
    );
}

// The B124 guard, re-pinned through the shared merge: an arm that LEAVES
// contributes `Never`, not its synthesized void tail, so a `ret` arm beside a
// value arm is not a mismatch.
#[test]
fn a_diverging_if_arm_does_not_report_a_mismatch() {
    assert_compiles_and_runs(
        r#"

        fun pick(c: bool): i32 {
        	let value = if c { 1 } else { ret 0; };
        	value + 1
        }

        fun main() {
        	print(pick(true));
        	print(pick(false));
        }
        "#,
        "2\n0\n",
    );
}

// The control the rule was copied from: the same program written with `match`
// fails the same way, and has since before B163. One rule, two constructs —
// the messages differ only in what they name.
#[test]
fn the_match_control_refuses_the_same_mismatch() {
    assert_fails_with(
        r#"

        fun main() {
        	let c = false;
        	let mixed = match c { true => 1, false => "two" };
        	print(mixed);
        }
        "#,
        "match legs have mismatched types: expected i32, but got str instead.",
    );
}
