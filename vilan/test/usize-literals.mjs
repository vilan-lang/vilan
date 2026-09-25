function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
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
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $f = null;
	if (wrapped < 0) {
		$f = wrapped + modulus;
	} else {
		$f = wrapped;
	}
	return $f;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $k = null;
	if (truncated > 0) {
		$k = truncated;
	} else {
		$k = 0;
	}
	return $k;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $g = null;
	if (wrapped >= half) {
		$g = wrapped - modulus;
	} else {
		$g = wrapped;
	}
	return $g;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function take(count) {
	return count;
}
function zero() {
	return 0;
}
function $c(value) {
	return __clone(value);
}
function $d(self) {
	const $e = self;
	return $e[0] === 0;
}
function $h(self, fallback) {
	const $i = self;
	let $j = null;
	if ($i[0] === 0) {
		const x = __clone($i[1]);
		$j = x;
	} else {
		$j = __clone(fallback);
	}
	return $j;
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
const through = $c(5);
console.log("" + through);
const bare = 0;
console.log("" + take(bare));
const found = [ 0, 0 ];
console.log($d(found));
const at = 1;
console.log(__at(letters, 0));
console.log(__at(letters, at));
console.log($h(__list_get(letters, as_i32(at)), "none"));
const length = as_usize(letters.length);
console.log("" + length);
let index = length;
while (index > 0) {
	index = index - 1;
	console.log(__at(letters, index));
}
