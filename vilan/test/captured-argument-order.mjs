function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $b = null;
	if (truncated > 0) {
		$b = truncated;
	} else {
		$b = 0;
	}
	return $b;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function measured(self, extra) {
	return self[0].length + as_usize(extra);
}
function size(words2, extra) {
	return words2[0].length + as_usize(extra);
}
function sized(words2, extra) {
	return words2[0].length + as_usize(unwrap_or(extra, 0));
}
function traced(first, words2, extra) {
	return words2[0].length + as_usize(first) + as_usize(extra);
}
function note(text) {
	console.log(text);
	return 0;
}
function counted(count2, extra) {
	return count2 + extra;
}
function unwrap_or(self, fallback) {
	const $e = self;
	let $f = null;
	if ($e[0] === 0) {
		const x = __clone($e[1]);
		$f = x;
	} else {
		$f = __clone(fallback);
	}
	return $f;
}
let words = [ [ "a" ] ];
let count = 0;
const swap = () => {
	words = [ [ "x", "y", "z" ] ];
	count = count + 1;
	return 0;
};
const $a = swap();
console.log(String(measured(words, $a)));
words = [ [ "a" ] ];
const $c = note("first");
const $d = swap();
console.log(String(traced($c, words, $d)));
console.log(String(size(words, 0)));
console.log(String(sized(words, [ 0, 1 ])));
count = 0;
console.log(String(counted(count, swap())));
let plain = [ [ "a" ] ];
const $g = plain;
plain = [ [ "x", "y" ] ];
console.log(String(size($g, 0)));
//# sourceMappingURL=captured-argument-order.mjs.map
