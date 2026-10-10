function __dbg(write, location, entries) {
	if (entries.length === 0) {
		write("[" + location + "]");
		return;
	}
	for (const entry of entries) {
		const head = "[" + location + "] " + entry[0] + " = ";
		write(head + __dbg_layout(entry[1], __dbg_width(head), 0));
	}
}
function __dbg_value(write, location, text, show, value) {
	__dbg(write, location, [ [ text, show(value) ] ]);
	return value;
}
function __dbg_values(write, location, texts, shows, values, spread) {
	__dbg(write, location, values.map((value, index) => [ texts[index], shows[index](value) ]));
	return spread ? values.flatMap((value, index) => spread[index] ? value : [ value ]) : values;
}
function __dbg_group(open, close, padded, entries, fill) {
	return { o: open, c: close, p: padded, e: entries, f: fill === true };
}
function __dbg_list(items, show, fill) {
	const entries = [];
	const shown = Math.min(items.length, 100);
	for (let index = 0; index < shown; index++) entries.push([ "", show(items[index]) ]);
	if (items.length > shown) entries.push([ "", "… " + (items.length - shown) + " more" ]);
	return __dbg_group("[", "]", false, entries, fill);
}
const __dbg_seen = [];
function __dbg_shared(cell, show) {
	if (__dbg_seen.includes(cell)) return "<cycle>";
	__dbg_seen.push(cell);
	try {
		return __dbg_group("Shared(", ")", false, [ [ "", show(cell.v) ] ]);
	} finally {
		__dbg_seen.pop();
	}
}
function __dbg_members(open, items, show, fill) {
	const entries = [];
	for (const item of items) {
		if (entries.length === 100) {
			entries.push([ "", "… " + (items.length - 100) + " more" ]);
			break;
		}
		entries.push(show(item));
	}
	return __dbg_group(open, "}", true, entries, fill);
}
function __dbg_map(open, table, showKey, showValue, keyWidth, valueWidth) {
	const items = Array.from(table.values());
	return __dbg_members(open, items, (pair) => {
		const key = keyWidth === 1 ? pair[0] : pair.slice(0, keyWidth);
		const value = valueWidth === 1 ? pair[keyWidth] : pair.slice(keyWidth, keyWidth + valueWidth);
		return [ __dbg_flat(showKey(key)) + " => ", showValue(value) ];
	});
}
function __dbg_set(open, table, show, fill) {
	return __dbg_members(open, Array.from(table.values()), (item) => [ "", show(item) ], fill);
}
function __dbg_str(text) {
	let out = "\"";
	for (const character of text) {
		if (character === "\\") out += "\\\\";
		else if (character === "\"") out += "\\\"";
		else if (character === "\n") out += "\\n";
		else if (character === "\t") out += "\\t";
		else if (character === "\r") out += "\\r";
		else if (character === "\0") out += "\\0";
		else out += character;
	}
	return out + "\"";
}
function __dbg_float(value) {
	if (Object.is(value, -0)) return "-0.0";
	const text = String(value);
	return Number.isInteger(value) && !text.includes("e") ? text + ".0" : text;
}
function __dbg_width(text) {
	let width = 0;
	for (const _ of text) width++;
	return width;
}
function __dbg_flat(document) {
	if (typeof document === "string") return document;
	if (document.e.length === 0) return document.o + document.c;
	const inner = document.e.map((entry) => entry[0] + __dbg_flat(entry[1])).join(", ");
	return document.p ? document.o + " " + inner + " " + document.c : document.o + inner + document.c;
}
function __dbg_layout(document, column, indent) {
	const flat = __dbg_flat(document);
	if (typeof document === "string" || document.e.length === 0 || column + __dbg_width(flat) <= 80) return flat;
	const pad = " ".repeat(indent + 2);
	let out = document.o + "\n";
	if (document.f) {
		let line = "";
		for (const entry of document.e) {
			const text = entry[0] + __dbg_flat(entry[1]) + ",";
			if (line === "") line = pad + text;
			else if (__dbg_width(line) + 1 + __dbg_width(text) > 80) {
				out += line + "\n";
				line = pad + text;
			} else line += " " + text;
		}
		return out + line + "\n" + " ".repeat(indent) + document.c;
	}
	for (const entry of document.e) out += pad + entry[0] + __dbg_layout(entry[1], __dbg_width(pad + entry[0]), indent + 2) + ",\n";
	return out + " ".repeat(indent) + document.c;
}
function __enum_trap(name, value) {
	throw name + ": " + JSON.stringify(value) + " is not one of its values";
}
function describe(order) {
	const $a = order;
	let $b = null;
	if ($a === -1) {
		$b = "less";
	} else if ($a === 0) {
		$b = "equal";
	} else if ($a === 1) {
		$b = "greater";
	} else {
		__enum_trap("Ordering", $a);
	}
	return $b;
}
function __show_Ordering(value) {
	if (value === -1) {
		return "Ordering::Less";
	}
	if (value === 0) {
		return "Ordering::Equal";
	}
	if (value === 1) {
		return "Ordering::Greater";
	}
	return "<?>";
}
console.log(__dbg_flat(__show_Ordering(-1)));
console.log(__dbg_flat(__show_Ordering(1)));
console.log(-1 < 0);
console.log(1 < 0);
console.log(0 === 0);
console.log(describe(1));
console.log(describe(-1));
