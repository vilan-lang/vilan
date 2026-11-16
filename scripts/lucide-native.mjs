#!/usr/bin/env node
// The Lucide icon package for `std::ui` (gfx-path.md S4, ruling R11): one
// generator, two steps, so the step that needs the upstream set is the only
// one that ever reads it.
//
//   node scripts/lucide-native.mjs generate <lucide-static package dir> <out dir>
//       Reads the upstream SVG set (an extracted `lucide-static` tarball at
//       the version pinned below, the `package/` directory), flattens every
//       icon into ONE path-data string in its 24-unit box, and writes
//       `<out>/icons.tsv` (the DATA: one icon per line) and
//       `<out>/src/icons/lib.vl` (the Vilan source, rendered from the data).
//
//   node scripts/lucide-native.mjs render <icons.tsv>
//       Prints the Vilan source the data renders to. The source is a pure
//       function of the data: `crates/vilan-cli/tests/lucide_package.rs`
//       holds the committed `lib.vl` to this output, and the GPU crate's
//       sweep (`crates/vilan-rt-gfx/src/path/lucide_sweep.rs`) reads the
//       data and holds it to the source's names.
//
// Regenerating at the pinned version is a no-op: the output is sorted by
// name, numbers have one spelling, and nothing records a time. A new
// version is a deliberate change: fetch it, check the tarball against the
// registry's integrity, bump UPSTREAM below, regenerate, and read the
// sweep's numbers again. `vilan/packages/lucide/README.md` has the steps.
//
// What it flattens: `path`, `circle`, `rect` (with and without `rx`/`ry`),
// `line`, `polyline`, `polygon` and `ellipse`, each into path data that
// draws what the element draws, joined in document order. The paint is the
// root's: Lucide's is a stroke 2 wide with round joins and caps and no fill.
// Anything else FAILS the generation with the icon's name: a transform, a
// group, opacity, a fill, a per-element stroke, an attribute it does not
// know. It never guesses. One fill is accepted because it changes no pixel:
// `fill="currentColor"` on a CIRCLE whose radius is at most half the stroke
// width (Lucide's dots), whose disk the stroke already covers whole (the
// stroke reaches `width / 2` inside the circle, past its centre). The fill
// is dropped and the circle stroked as every other element is.

import { readFileSync, readdirSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";

// The upstream this package is generated from. Bump all three together.
const UPSTREAM = {
    name: "lucide-static",
    version: "1.64.0",
    integrity:
        "sha512-UxWmMteOXbKHI5E4f2qVtxCAAsI9gsInYK1lNw0nrOA0w4FkJeOVZ+UOV6aGA2377FZe3qyiFYq3lqjBKZ6OuA==",
};

// Vilan's reserved words (`crates/vilan-core/src/lexing.rs`'s `KEYWORDS`)
// and the names with a fixed meaning. An icon named by one gets a trailing
// underscore (`type` -> `type_`); `vilan check` over the package is the
// gate that catches a word this list has not learned yet.
const RESERVED = new Set([
    "async", "await", "const", "css", "else", "enum", "export", "external",
    "false", "for", "fun", "if", "impl", "import", "in", "is", "let", "macro",
    "match", "mod", "mut", "null", "ret", "struct", "trait", "type", "true",
    "use", "self", "Self", "void", "super", "pkg", "std",
]);

/// An upstream name (`arrow-up-right`) as a Vilan function name
/// (`arrow_up_right`). A name that is a reserved word gets a trailing
/// underscore; a name that starts with a digit gets the prefix `icon_`.
function identifier(name) {
    if (!/^[a-z0-9]+(-[a-z0-9]+)*$/.test(name)) {
        throw new Error(`${name}: not a lowercase, hyphenated name`);
    }
    let result = name.replaceAll("-", "_");
    if (/^[0-9]/.test(result)) {
        result = `icon_${result}`;
    }
    if (RESERVED.has(result)) {
        result = `${result}_`;
    }
    return result;
}

// --- Numbers ---------------------------------------------------------------------

/// One spelling per number: rounded to six decimals (flattening a circle
/// adds and subtracts), no exponent, no negative zero.
function number(value) {
    if (!Number.isFinite(value)) {
        throw new Error(`not a finite number: ${value}`);
    }
    let rounded = Number(value.toFixed(6));
    if (Object.is(rounded, -0)) {
        rounded = 0;
    }
    const text = String(rounded);
    if (/e/i.test(text)) {
        throw new Error(`a number this generator would spell with an exponent: ${value}`);
    }
    return text;
}

/// Numbers in path data: a space between two, none before a minus sign.
function numbers(values) {
    let text = "";
    for (const value of values) {
        const spelled = typeof value === "string" ? value : number(value);
        if (text !== "" && !spelled.startsWith("-")) {
            text += " ";
        }
        text += spelled;
    }
    return text;
}

// --- Path data ---------------------------------------------------------------------

const ARGUMENTS = { m: 2, l: 2, h: 1, v: 1, c: 6, s: 4, q: 4, t: 2, a: 7, z: 0 };

/// Parses SVG path data (SVG 2's grammar) into command groups:
/// `{ letter, values }`, where `values` holds every argument of the letter
/// and its implicit repeats, arc flags as "0"/"1". Throws on anything the
/// grammar refuses, naming the byte.
function parsePathData(data) {
    const groups = [];
    let at = 0;
    const fail = (why) => {
        throw new Error(`path data ${JSON.stringify(data)}: ${why} at byte ${at}`);
    };
    const skipSpace = () => {
        while (at < data.length && /[\s]/.test(data[at])) at += 1;
    };
    const skipSeparator = () => {
        skipSpace();
        if (data[at] === ",") {
            at += 1;
            skipSpace();
        }
    };
    const readNumber = () => {
        const match = /^[+-]?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?/.exec(data.slice(at));
        if (match === null) fail("expected a number");
        at += match[0].length;
        return Number(match[0]);
    };
    const readFlag = () => {
        if (data[at] !== "0" && data[at] !== "1") fail("expected an arc flag (0 or 1)");
        const flag = data[at];
        at += 1;
        return flag;
    };
    skipSpace();
    while (at < data.length) {
        const letter = data[at];
        const lower = letter.toLowerCase();
        if (!(lower in ARGUMENTS)) fail(`expected a command letter, found ${JSON.stringify(letter)}`);
        if (groups.length === 0 && lower !== "m") fail("path data must start with a move");
        at += 1;
        skipSpace();
        const values = [];
        const count = ARGUMENTS[lower];
        if (count > 0) {
            do {
                for (let index = 0; index < count; index += 1) {
                    if (lower === "a" && (index === 3 || index === 4)) {
                        values.push(readFlag());
                    } else {
                        values.push(readNumber());
                    }
                    skipSeparator();
                }
            } while (at < data.length && /[0-9.+-]/.test(data[at]));
        }
        groups.push({ letter, values });
        skipSpace();
    }
    if (groups.length === 0) fail("empty path data");
    return groups;
}

/// Path data for one element's `d`, ready to follow another element's: a
/// LEADING relative move (`m`) is relative to the origin in its own
/// element, but would be relative to the previous element's end point once
/// joined, so it is spelled absolute; the pairs after it stay relative
/// (SVG's rule for a first `m`), so they become an explicit `l`.
function pathElementData(d) {
    const groups = parsePathData(d);
    const [first, ...rest] = groups;
    let text = "";
    if (first.letter === "m") {
        text += `M${numbers(first.values.slice(0, 2))}`;
        if (first.values.length > 2) {
            text += `l${numbers(first.values.slice(2))}`;
        }
    } else {
        text += `${first.letter}${numbers(first.values)}`;
    }
    for (const group of rest) {
        text += `${group.letter}${numbers(group.values)}`;
    }
    return text;
}

// --- SVG ---------------------------------------------------------------------------

/// The one root and its children of a Lucide SVG: `{ attributes, children:
/// [{ tag, attributes }] }`. Refuses anything but self-closing children
/// with quoted attributes, a comment and whitespace (no groups, no text).
function readSvg(text, name) {
    const refuse = (why) => {
        throw new Error(`${name}: ${why}`);
    };
    const body = text.replace(/<!--[\s\S]*?-->/g, "").trim();
    const attributePattern = /([A-Za-z_:][\w:.-]*)\s*=\s*"([^"]*)"/g;
    const readAttributes = (source) => {
        const attributes = new Map();
        const leftover = source.replace(attributePattern, (_, key, value) => {
            if (attributes.has(key)) refuse(`attribute ${key} given twice`);
            attributes.set(key, value);
            return "";
        });
        if (leftover.trim() !== "") refuse(`unreadable attributes: ${JSON.stringify(leftover.trim())}`);
        return attributes;
    };
    const root = /^<svg\b([^>]*)>([\s\S]*)<\/svg>$/.exec(body);
    if (root === null) refuse("not one <svg> element");
    const children = [];
    const childPattern = /<([a-z]+)\b([^>]*?)\/>/g;
    const between = root[2].replace(childPattern, (_, tag, attributes) => {
        children.push({ tag, attributes: readAttributes(attributes) });
        return "";
    });
    if (between.trim() !== "") {
        refuse(`content other than self-closing elements: ${JSON.stringify(between.trim().slice(0, 80))}`);
    }
    return { attributes: readAttributes(root[1]), children };
}

/// A plain number attribute (no unit, no percentage).
function numberAttribute(attributes, key, name, fallback) {
    if (!attributes.has(key)) {
        if (fallback === undefined) throw new Error(`${name}: missing ${key}`);
        return fallback;
    }
    const text = attributes.get(key).trim();
    if (!/^[+-]?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?$/.test(text)) {
        throw new Error(`${name}: ${key}=${JSON.stringify(text)} is not a plain number`);
    }
    return Number(text);
}

/// A list of points (`polyline`, `polygon`).
function pointsAttribute(attributes, name) {
    const text = attributes.get("points") ?? "";
    const values = text.trim().split(/[\s,]+/).filter((part) => part !== "").map(Number);
    if (values.length < 4 || values.length % 2 !== 0 || values.some((value) => !Number.isFinite(value))) {
        throw new Error(`${name}: points=${JSON.stringify(text)} is not a list of two or more points`);
    }
    return values;
}

const ROOT_ATTRIBUTES = new Set([
    "class", "xmlns", "width", "height", "viewBox", "fill", "stroke",
    "stroke-width", "stroke-linecap", "stroke-linejoin",
]);

const ELEMENT_ATTRIBUTES = {
    path: ["d"],
    circle: ["cx", "cy", "r"],
    ellipse: ["cx", "cy", "rx", "ry"],
    rect: ["x", "y", "width", "height", "rx", "ry"],
    line: ["x1", "y1", "x2", "y2"],
    polyline: ["points"],
    polygon: ["points"],
};

const JOINS = { round: "Round", miter: "Miter", bevel: "Bevel" };
const CAPS = { round: "Round", butt: "Butt", square: "Square" };

/// The root's paint: `{ kind: "stroke", width, join, cap }` or `{ kind:
/// "fill", rule }`, and the box.
function rootPaint(attributes, name) {
    for (const key of attributes.keys()) {
        if (!ROOT_ATTRIBUTES.has(key)) throw new Error(`${name}: the root's ${key} is not handled`);
    }
    const viewBox = (attributes.get("viewBox") ?? "").trim().split(/[\s,]+/).map(Number);
    if (viewBox.length !== 4 || viewBox[0] !== 0 || viewBox[1] !== 0 || viewBox[2] !== viewBox[3] || !(viewBox[2] > 0)) {
        throw new Error(`${name}: viewBox ${JSON.stringify(attributes.get("viewBox"))} is not a square box at the origin`);
    }
    const box = viewBox[2];
    const fill = attributes.get("fill") ?? "black";
    const stroke = attributes.get("stroke") ?? "none";
    if (fill === "none" && stroke === "currentColor") {
        const width = numberAttribute(attributes, "stroke-width", name, 1);
        const join = attributes.get("stroke-linejoin") ?? "miter";
        const cap = attributes.get("stroke-linecap") ?? "butt";
        if (!(join in JOINS)) throw new Error(`${name}: stroke-linejoin ${join} is not handled`);
        if (!(cap in CAPS)) throw new Error(`${name}: stroke-linecap ${cap} is not handled`);
        return { box, paint: { kind: "stroke", width, join, cap } };
    }
    if (stroke === "none" && fill === "currentColor") {
        return { box, paint: { kind: "fill", rule: "nonzero" } };
    }
    throw new Error(`${name}: fill=${fill} with stroke=${stroke} is not one paint this generator handles`);
}

/// One element as path data.
function elementData(child, paint, name) {
    const { tag, attributes } = child;
    const known = ELEMENT_ATTRIBUTES[tag];
    if (known === undefined) throw new Error(`${name}: <${tag}> is not handled`);
    for (const key of attributes.keys()) {
        if (known.includes(key)) continue;
        // Lucide's filled dots: a fill the stroke already covers whole.
        if (
            key === "fill" &&
            tag === "circle" &&
            attributes.get("fill") === "currentColor" &&
            paint.kind === "stroke" &&
            numberAttribute(attributes, "r", name) <= paint.width / 2
        ) {
            continue;
        }
        throw new Error(`${name}: <${tag}> ${key}=${JSON.stringify(attributes.get(key))} is not handled`);
    }
    const value = (key, fallback) => numberAttribute(attributes, key, name, fallback);
    switch (tag) {
        case "path":
            return pathElementData(attributes.get("d") ?? "");
        case "circle":
            return ellipseData(value("cx", 0), value("cy", 0), value("r"), value("r"), name);
        case "ellipse":
            return ellipseData(value("cx", 0), value("cy", 0), value("rx"), value("ry"), name);
        case "line":
            return `M${numbers([value("x1", 0), value("y1", 0)])}L${numbers([value("x2", 0), value("y2", 0)])}`;
        case "polyline":
        case "polygon": {
            const points = pointsAttribute(attributes, name);
            const close = tag === "polygon" ? "Z" : "";
            return `M${numbers(points.slice(0, 2))}L${numbers(points.slice(2))}${close}`;
        }
        case "rect":
            return rectData(attributes, name);
        default:
            throw new Error(`${name}: <${tag}> is not handled`);
    }
}

/// An ellipse (a circle is one) as two arcs from its rightmost point,
/// clockwise in the y-down box, closed.
function ellipseData(cx, cy, rx, ry, name) {
    if (!(rx > 0 && ry > 0)) throw new Error(`${name}: an ellipse or circle of radius ${rx}, ${ry} draws nothing`);
    return (
        `M${numbers([cx + rx, cy])}` +
        `a${numbers([rx, ry, 0, "1", "1", -2 * rx, 0])}` +
        `a${numbers([rx, ry, 0, "1", "1", 2 * rx, 0])}Z`
    );
}

/// A rectangle, its corners rounded by SVG 2's rule: one of `rx` and `ry`
/// given stands for both, and each is at most half its side.
function rectData(attributes, name) {
    const value = (key, fallback) => numberAttribute(attributes, key, name, fallback);
    const x = value("x", 0);
    const y = value("y", 0);
    const width = value("width");
    const height = value("height");
    if (!(width > 0 && height > 0)) throw new Error(`${name}: a rect ${width} by ${height} draws nothing`);
    let rx = attributes.has("rx") ? value("rx") : undefined;
    let ry = attributes.has("ry") ? value("ry") : undefined;
    if (rx === undefined) rx = ry ?? 0;
    if (ry === undefined) ry = rx;
    if (rx < 0 || ry < 0) throw new Error(`${name}: a rect's radius is negative`);
    rx = Math.min(rx, width / 2);
    ry = Math.min(ry, height / 2);
    if (rx === 0 || ry === 0) {
        return `M${numbers([x, y])}h${numbers([width])}v${numbers([height])}h${numbers([-width])}Z`;
    }
    const across = width - 2 * rx;
    const down = height - 2 * ry;
    let text = `M${numbers([x + rx, y])}`;
    if (across > 0) text += `h${numbers([across])}`;
    text += `a${numbers([rx, ry, 0, "0", "1", rx, ry])}`;
    if (down > 0) text += `v${numbers([down])}`;
    text += `a${numbers([rx, ry, 0, "0", "1", -rx, ry])}`;
    if (across > 0) text += `h${numbers([-across])}`;
    text += `a${numbers([rx, ry, 0, "0", "1", -rx, -ry])}`;
    if (down > 0) text += `v${numbers([-down])}`;
    text += `a${numbers([rx, ry, 0, "0", "1", rx, -ry])}Z`;
    return text;
}

// --- The data file -----------------------------------------------------------------

/// A paint as the data file spells it.
function paintField(paint) {
    if (paint.kind === "stroke") {
        return `stroke width=${number(paint.width)} join=${paint.join} cap=${paint.cap}`;
    }
    return `fill rule=${paint.rule}`;
}

function parsePaintField(field, line) {
    const words = field.split(" ");
    const pairs = Object.fromEntries(words.slice(1).map((word) => word.split("=")));
    if (words[0] === "stroke" && pairs.width !== undefined && pairs.join in JOINS && pairs.cap in CAPS) {
        return { kind: "stroke", width: Number(pairs.width), join: pairs.join, cap: pairs.cap };
    }
    if (words[0] === "fill" && (pairs.rule === "nonzero" || pairs.rule === "evenodd")) {
        return { kind: "fill", rule: pairs.rule };
    }
    throw new Error(`line ${line}: unreadable paint ${JSON.stringify(field)}`);
}

const DATA_HEADER = [
    `# Lucide's icons as path data, one per line: the upstream name, the Vilan`,
    `# name, the box, the paint, the data. GENERATED by scripts/lucide-native.mjs`,
    `# from ${UPSTREAM.name} ${UPSTREAM.version} (ISC; LICENSE beside this file),`,
    `# tarball integrity ${UPSTREAM.integrity}.`,
    `# Do not edit: regenerate (README.md says how). Tab-separated.`,
];

function writeData(icons) {
    const lines = [...DATA_HEADER];
    for (const icon of icons) {
        lines.push([icon.name, icon.identifier, number(icon.box), paintField(icon.paint), icon.data].join("\t"));
    }
    return `${lines.join("\n")}\n`;
}

function readData(text) {
    const icons = [];
    text.replaceAll("\r\n", "\n").split("\n").forEach((line, index) => {
        if (line === "" || line.startsWith("#")) return;
        const fields = line.split("\t");
        if (fields.length !== 5) throw new Error(`line ${index + 1}: ${fields.length} fields, not 5`);
        const [name, identifier, box, paint, data] = fields;
        icons.push({ name, identifier, box: Number(box), paint: parsePaintField(paint, index + 1), data });
    });
    return icons;
}

// --- The Vilan source --------------------------------------------------------------

/// A box or a width as a Vilan `f64` literal (`24.0`, `1.5`).
function float(value) {
    const text = number(value);
    return text.includes(".") ? text : `${text}.0`;
}

function constructor(icon) {
    const data = JSON.stringify(icon.data);
    const box = float(icon.box);
    const paint = icon.paint;
    if (paint.kind === "stroke") {
        if (paint.join === "round" && paint.cap === "round") {
            return { call: `Icon::stroked(${data}, ${box}, ${float(paint.width)})`, uses: [] };
        }
        let stroke = `Stroke::new(${float(paint.width)})`;
        if (paint.join !== "round") stroke += `.join(Join::${JOINS[paint.join]})`;
        if (paint.cap !== "round") stroke += `.cap(Cap::${CAPS[paint.cap]})`;
        const uses = ["PathPaint", "Stroke"];
        if (paint.join !== "round") uses.push("Join");
        if (paint.cap !== "round") uses.push("Cap");
        return { call: `Icon::new(${data}, ${box}, PathPaint::Stroke(${stroke}))`, uses };
    }
    if (paint.rule === "nonzero") {
        return { call: `Icon::filled(${data}, ${box})`, uses: [] };
    }
    return { call: `Icon::new(${data}, ${box}, PathPaint::Fill(FillRule::EvenOdd))`, uses: ["FillRule", "PathPaint"] };
}

function render(icons) {
    const built = icons.map((icon) => ({ icon, ...constructor(icon) }));
    const gfx = [...new Set(built.flatMap((entry) => entry.uses))].sort();
    const lines = [
        `// GENERATED by scripts/lucide-native.mjs from ${UPSTREAM.name} ${UPSTREAM.version}`,
        `// (ISC, with Feather's MIT notice: vilan/packages/lucide/LICENSE). Do not edit:`,
        `// regenerate (vilan/packages/lucide/README.md). ${icons.length} icons.`,
        `// Tarball integrity ${UPSTREAM.integrity}.`,
        ``,
    ];
    if (gfx.length > 0) {
        lines.push(`import std::gfx::{ ${gfx.join(", ")} };`);
    }
    lines.push(`import std::ui::Icon;`);
    for (const { icon, call } of built) {
        lines.push(``);
        if (icon.identifier !== icon.name.replaceAll("-", "_")) {
            lines.push(`/// Lucide's \`${icon.name}\`.`);
        }
        lines.push(`export fun ${icon.identifier}(): Icon {`);
        lines.push(`\t${call}`);
        lines.push(`}`);
    }
    return `${lines.join("\n")}\n`;
}

// --- Generation --------------------------------------------------------------------

function generate(upstream, out) {
    const manifest = JSON.parse(readFileSync(join(upstream, "package.json"), "utf8"));
    if (manifest.name !== UPSTREAM.name || manifest.version !== UPSTREAM.version) {
        throw new Error(`${upstream} is ${manifest.name} ${manifest.version}, not the pinned ${UPSTREAM.name} ${UPSTREAM.version}`);
    }
    // The set's own names: `icon-nodes.json` lists every icon once. The
    // `icons/` directory also carries a file per deprecated ALIAS (the same
    // drawing under an old name); aliases are left out on purpose.
    const names = Object.keys(JSON.parse(readFileSync(join(upstream, "icon-nodes.json"), "utf8"))).sort();
    const files = new Set(readdirSync(join(upstream, "icons")).filter((file) => file.endsWith(".svg")));
    const icons = [];
    const refused = [];
    const seen = new Map();
    for (const name of names) {
        try {
            if (!files.has(`${name}.svg`)) throw new Error(`${name}: no icons/${name}.svg`);
            const svg = readSvg(readFileSync(join(upstream, "icons", `${name}.svg`), "utf8"), name);
            const { box, paint } = rootPaint(svg.attributes, name);
            if (svg.children.length === 0) throw new Error(`${name}: no elements`);
            const data = svg.children.map((child) => elementData(child, paint, name)).join("");
            // The data must read back as the same commands: a flattening
            // that wrote something the grammar refuses fails here.
            parsePathData(data);
            const id = identifier(name);
            if (seen.has(id)) throw new Error(`${name}: its name ${id} is also ${seen.get(id)}'s`);
            seen.set(id, name);
            icons.push({ name, identifier: id, box, paint, data });
        } catch (error) {
            refused.push(error.message);
        }
    }
    if (refused.length > 0) {
        process.stderr.write(`refused ${refused.length} icon(s); nothing written:\n${refused.map((line) => `  ${line}\n`).join("")}`);
        process.exit(1);
    }
    mkdirSync(join(out, "src", "icons"), { recursive: true });
    const data = writeData(icons);
    writeFileSync(join(out, "icons.tsv"), data);
    writeFileSync(join(out, "src", "icons", "lib.vl"), render(readData(data)));
    process.stdout.write(`${icons.length} icons from ${UPSTREAM.name} ${UPSTREAM.version} (${files.size - names.length} alias files left out)\n`);
}

const [command, ...rest] = process.argv.slice(2);
if (command === "generate" && rest.length === 2) {
    generate(rest[0], rest[1]);
} else if (command === "render" && rest.length === 1) {
    process.stdout.write(render(readData(readFileSync(rest[0], "utf8"))));
} else {
    process.stderr.write(
        "usage: node scripts/lucide-native.mjs generate <lucide-static package dir> <out dir>\n" +
            "       node scripts/lucide-native.mjs render <icons.tsv>\n",
    );
    process.exit(2);
}
