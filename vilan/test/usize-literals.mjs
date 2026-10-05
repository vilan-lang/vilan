function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __list_get(list, index) {
	return index >= 0 && index < list.length ? [ 0, __clone(list[index]) ] : [ 1 ];
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $d = null;
	if (wrapped < 0) {
		$d = wrapped + modulus;
	} else {
		$d = wrapped;
	}
	return $d;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $f = null;
	if (truncated > 0) {
		$f = truncated;
	} else {
		$f = 0;
	}
	return $f;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $e = null;
	if (wrapped >= half) {
		$e = wrapped - modulus;
	} else {
		$e = wrapped;
	}
	return $e;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function as_usize2(self) {
	return self;
}
function take(count) {
	return count;
}
function zero() {
	return 0;
}
function identity(value) {
	return __clone(value);
}
function is_some(self) {
	const $c = self;
	return $c[0] === 0;
}
function unwrap_or(self, fallback) {
	const $g = self;
	let $h = null;
	if ($g[0] === 0) {
		const x = __clone($g[1]);
		$h = x;
	} else {
		$h = __clone(fallback);
	}
	return $h;
}
const letters = [ "a", "b", "c" ];
console.log("" + take(3));
const annotated = 0;
console.log("" + annotated);
const four = 4;
const right = four + 1;
const left = 1 + four;
console.log("" + right + " " + left);
const positions = [ 0, 1, 2 ];
console.log("" + positions.length);
const holder = [ 0 ];
console.log("" + holder[0]);
console.log("" + zero());
console.log(four > 0);
let counted = 4;
counted = counted - 1;
console.log("" + counted);
const $a = four;
let $b = null;
if ($a === 0) {
	$b = "zero";
} else if ($a === 4) {
	$b = "four";
} else {
	$b = "more";
}
const named = $b;
console.log(named);
const pair = [ 0, "a" ];
console.log("" + pair[0] + " " + pair[1]);
const doubled = (n) => {
	return n * 2;
};
console.log("" + doubled(7));
const through = identity(5);
console.log("" + through);
const bare = 0;
console.log("" + take(bare));
const found = [ 0, 0 ];
console.log(is_some(found));
const at = 1;
console.log(__at(letters, 0, "usize-literals.vl:76:8"));
console.log(__at(letters, at, "usize-literals.vl:77:8"));
console.log(unwrap_or(__list_get(letters, as_usize(as_i32(at))), "none"));
const length = as_usize2(letters.length);
console.log("" + length);
let index = length;
while (index > 0) {
	index = index - 1;
	console.log(__at(letters, index, "usize-literals.vl:87:9"));
}
