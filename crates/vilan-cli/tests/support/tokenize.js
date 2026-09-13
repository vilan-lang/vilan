// The TextMate grammar's own tokeniser, run the way VS Code runs it (tracker
// E163). `grammar_sync.rs`'s E161/E162/E164 pins used to assert each rule's
// REGEX plus the order the rules sit in — which together decide an outcome
// without ever producing one. This helper produces the outcome: it loads
// `editors/vscode/syntaxes/vilan.tmLanguage.json` into `vscode-textmate` with
// the same oniguruma engine VS Code uses (`vscode-oniguruma`'s wasm build),
// tokenises a program, and prints the scope stack every character ends up
// with. A scope pin can then say "`</span>` after text is a TAG", which is the
// claim the item was filed about.
//
// Both packages are `editors/vscode` devDependencies: nothing ships them (the
// vsix bundles `dependencies` only, `.vscodeignore` drops `node_modules/`),
// and `npm ci --prefix editors/vscode` is what puts them on disk — a step in
// ci.yml's `test` job and release.yml's `gate` job.
//
//   node crates/vilan-cli/tests/support/tokenize.js  < program.vl
//
//     VILAN_GRAMMAR    the .tmLanguage.json to load     (required)
//     VILAN_MODULES    the node_modules holding the two packages (required)
//     VILAN_SCOPE      the grammar's own scope name     (default source.vilan)
//
// Output is one line per token, tab separated, in tokenisation order:
//
//     <line>\t<start>\t<end>\t<JSON-quoted text>\t<scope> <scope> …
//
// Line and column are zero-based; the text is JSON-quoted so a token of
// whitespace, a tab or a quote survives the line protocol intact.

const fs = require("fs");
const path = require("path");

function required(name) {
    const value = process.env[name];
    if (!value) {
        console.error(`tokenize.js: ${name} is not set`);
        process.exit(2);
    }
    return value;
}

const grammarPath = required("VILAN_GRAMMAR");
const modules = path.resolve(required("VILAN_MODULES"));
const scopeName = process.env.VILAN_SCOPE || "source.vilan";

const vsctm = require(path.join(modules, "vscode-textmate"));
const oniguruma = require(path.join(modules, "vscode-oniguruma"));
const wasm = fs.readFileSync(
    path.join(modules, "vscode-oniguruma", "release", "onig.wasm"),
);

const onigLib = oniguruma.loadWASM(wasm.buffer.slice(wasm.byteOffset, wasm.byteOffset + wasm.byteLength)).then(() => ({
    createOnigScanner: (patterns) => new oniguruma.OnigScanner(patterns),
    createOnigString: (text) => new oniguruma.OnigString(text),
}));

const registry = new vsctm.Registry({
    onigLib,
    loadGrammar: (requested) => {
        if (requested !== scopeName) {
            return Promise.resolve(null);
        }
        const text = fs.readFileSync(grammarPath).toString();
        return Promise.resolve(vsctm.parseRawGrammar(text, grammarPath));
    },
});

const source = fs.readFileSync(0, "utf8");

registry
    .loadGrammar(scopeName)
    .then((grammar) => {
        if (!grammar) {
            console.error(`tokenize.js: no grammar for ${scopeName} in ${grammarPath}`);
            process.exit(2);
        }
        // `\r\n` is split away rather than tokenised: a grammar is line-based
        // and a trailing `\r` would show up as a token of its own on windows.
        const lines = source.split("\n").map((line) => line.replace(/\r$/, ""));
        let stack = vsctm.INITIAL;
        const out = [];
        lines.forEach((line, index) => {
            const result = grammar.tokenizeLine(line, stack);
            for (const token of result.tokens) {
                const text = line.slice(token.startIndex, token.endIndex);
                out.push(
                    [
                        index,
                        token.startIndex,
                        token.endIndex,
                        JSON.stringify(text),
                        token.scopes.join(" "),
                    ].join("\t"),
                );
            }
            stack = result.ruleStack;
        });
        process.stdout.write(out.join("\n") + (out.length ? "\n" : ""));
    })
    .catch((error) => {
        console.error(`tokenize.js: ${error && error.stack ? error.stack : error}`);
        process.exit(2);
    });
