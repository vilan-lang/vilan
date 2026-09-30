// The playground compiler's smoke (N134): RUN the artifact the `wasm` leg just
// built, the way the page will, before it can be published.
//
//   node scripts/wasm-smoke.mjs <dir>   # <dir> holds wasm-bindgen's output:
//                                       # vilan_wasm.js + vilan_wasm_bg.wasm
//
// The leg used to prove only that the crate REACHES wasm32 and that the
// bindings generate. v0.41.0 published a compiler that loaded, reported its
// version, formatted and completed — and trapped on every compile, because one
// wall-clock read the analysis path took has no clock on wasm32 (B432). Nothing
// between the build and the release ran a compile; the website's deploy-time
// smoke did, after publication. These are that smoke's four claims, held where
// the artifact is made:
//
//   1. the module loads and its version is the crate's;
//   2. a compile answers JavaScript with no error diagnostic — an empty `main`
//      and a browser program that exercises std::ui and std::reactive;
//   3. `compile_for(.., "node")` compiles a process program;
//   4. `complete` offers a signal's members after `count.`.
//
// Plain node, no test framework: the glue is `--target web`, whose `init` takes
// the raw bytes, so no fetch and no browser are needed.
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { resolve } from "node:path";

const dir = process.argv[2];
if (!dir) {
	console.error("usage: node scripts/wasm-smoke.mjs <wasm-bindgen output dir>");
	process.exit(2);
}
const glue = await import(pathToFileURL(resolve(dir, "vilan_wasm.js")).href);
await glue.default({ module_or_path: readFileSync(resolve(dir, "vilan_wasm_bg.wasm")) });

let failed = false;
const fail = (message) => {
	failed = true;
	console.error(`wasm-smoke: FAILED - ${message}`);
};

// 1. The version is the crate's.
const manifest = readFileSync(new URL("../crates/vilan-wasm/Cargo.toml", import.meta.url), "utf8");
const expected = /^version = "([^"]+)"/m.exec(manifest)[1];
const version = glue.version();
if (version === expected) {
	console.log(`wasm-smoke: vilan ${version} loaded`);
} else {
	fail(`version() answered ${version}, the crate is ${expected}`);
}

// 2 + 3. Every compile answers js with no error diagnostic.
const programs = [
	["an empty main", "browser", "fun main() {}\n"],
	[
		"a browser program",
		"browser",
		`import std::reactive::Signal;
import std::ui::{ mount_root, view };

fun main() {
	mount_root("app", || {
		let count = Signal::new(0);
		view("div")
			.child(view("button").text("+1").on("click", || count.set(count.get() + 1)))
			.child(view("p").bind_text(count.derive(|n| i"clicked {n} times")))
	});
}
`,
	],
	[
		"a process program",
		"node",
		`import std::io::print;

fun main() {
	let names = ["a", "b", "c"];
	print(i"{names.len()} names, first {names.get(0usize).unwrap_or("none")}");
}
`,
	],
];
for (const [name, platform, source] of programs) {
	let result;
	try {
		result = platform === "node" ? glue.compile_for(source, platform) : glue.compile(source);
	} catch (error) {
		fail(`${name} threw ${error} - the compiler trapped (a panic aborts on wasm32; build with --keep-debug for the chain)`);
		continue;
	}
	const errors = result.diagnostics.filter((d) => d.severity === "error");
	if (!result.js || errors.length > 0) {
		fail(`${name} did not compile clean`);
		for (const d of errors) console.error(`  ${d.file}:${d.line + 1}:${d.column + 1} ${d.message}`);
	} else {
		console.log(`wasm-smoke: ${name}: ok (${result.js.length} B js${platform === "node" ? ", node leg" : ""})`);
	}
}

// 4. Completion answers from the retained analysis.
const counter = programs[1][2];
const lines = counter.split("\n");
const line = lines.findIndex((text) => text.includes("count.set("));
const character = lines[line].indexOf("count.set(") + "count.".length;
try {
	glue.compile(counter);
	const labels = glue.complete(counter, line, character).map((item) => {
		const label = item.label;
		item.free();
		return label;
	});
	for (const want of ["get", "set"]) {
		if (!labels.includes(want)) fail(`\`count.\` did not offer ${want}: ${labels.join(", ")}`);
	}
	if (!failed) console.log(`wasm-smoke: completion: ok (${labels.length} candidates after \`count.\`)`);
} catch (error) {
	fail(`complete threw ${error}`);
}

process.exit(failed ? 1 : 0);
