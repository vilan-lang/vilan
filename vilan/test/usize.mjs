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
function __hash(value) {
	return (typeof value === "object" && value !== null) ? JSON.stringify(value) : value;
}
function __json_kind(value) {
	if (value === null) return "null";
	if (Array.isArray(value)) return "array";
	return typeof value;
}
function __map_get(map, key) {
	return map.has(key) ? [ 0, __clone(map.get(key)) ] : [ 1 ];
}
function __try_parse_json(text) {
	try {
		return [ 0, JSON.parse(text) ];
	} catch (error) {
		return [ 1 ];
	}
}
function to_string(self) {
	return "" + self;
}
function hash(self) {
	return __hash(self);
}
function from_json(text) {
	const $l = __try_parse_json(text);
	let $m = null;
	if ($l[0] === 0) {
		const value = $l[1];
		$m = from_json_value(value);
	} else {
		$m = [ 1, "not valid JSON" ];
	}
	return $m;
}
function from_json_value(value) {
	let $n = null;
	if (__json_kind(value) === "number") {
		$n = [ 0, Number(value) ];
	} else {
		$n = [ 1, "expected a number" ];
	}
	return $n;
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $i = null;
	if (wrapped < 0) {
		$i = wrapped + modulus;
	} else {
		$i = wrapped;
	}
	return $i;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $j = null;
	if (wrapped >= half) {
		$j = wrapped - modulus;
	} else {
		$j = wrapped;
	}
	return $j;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(Math.trunc(widened));
}
function as_usize2(self) {
	const widened = Number(self);
	return Number(Math.trunc(widened));
}
function max_value() {
	return 9007199254740992;
}
function min_value() {
	return 0;
}
function rem(self, m) {
	return self - Math.trunc(self / m) * m;
}
function checked_sub(self, other) {
	let $c = null;
	if (self >= other) {
		$c = [ 0, self - other ];
	} else {
		$c = [ 1 ];
	}
	return $c;
}
function saturating_sub(self, other) {
	let $b = null;
	if (self >= other) {
		$b = self - other;
	} else {
		$b = 0;
	}
	return $b;
}
function as_u8(self) {
	const widened = Number(self);
	return Number(fold_unsigned(widened, 256));
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function as_usize3(self) {
	const widened = self;
	return Number(Math.trunc(widened));
}
function div(self, b) {
	return Math.trunc(self / b);
}
function $a(value, divisor) {
	return div(value, divisor);
}
function $d(self) {
	const $e = self;
	return $e[0] === 1;
}
function $f(self, fallback) {
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
function $k(value) {
	return to_string(value);
}
function $q() {
	const table = new Map();
	return [ table ];
}
function $r(self, key, value) {
	self[0].set(hash(key), [ __clone(key), __clone(value) ]);
}
function $s(self, key) {
	const $t = __map_get(self[0], hash(key));
	let $u = null;
	if ($t[0] === 0) {
		const entry = $t[1];
		$u = [ 0, __clone(entry[1]) ];
	} else {
		$u = [ 1 ];
	}
	return $u;
}
function $y(self) {
	let result = "[";
	let first = true;
	for (const element of self) {
		if (!(first)) {
			result = result + ",";
		}
		result = result + JSON.stringify(element);
		first = false;
	}
	return result + "]";
}
function $C(value) {
	let $D = null;
	if (__json_kind(value) !== "array") {
		return [ 1, "expected an array" ];
	}
	$D;
	let result = [  ];
	for (const element of value) {
		const $E = from_json_value(element);
		if ($E[0] === 1) {
			return $E;
		}
		result.push($E[1]);
	}
	return [ 0, result ];
}
function $z(text) {
	const $A = __try_parse_json(text);
	let $B = null;
	if ($A[0] === 0) {
		const value = $A[1];
		$B = $C(value);
	} else {
		$B = [ 1, "not valid JSON" ];
	}
	return $B;
}
const count = 42;
const step = 5;
const total = count + step;
const left = count - step;
const scaled = count * step;
const quotient = Math.trunc(count / step);
const remainder = rem(count, step);
console.log("" + total + " " + left + " " + scaled + " " + quotient + " " + remainder);
const halved = $a(count, 4);
console.log("" + halved);
console.log("" + (count > step) + " " + (count === 42) + " " + (count !== step));
const top = max_value();
const bottom = min_value();
console.log("" + top + " " + bottom);
const floor = saturating_sub(step, count);
console.log("" + floor);
console.log($d(checked_sub(step, count)));
const back = $f(checked_sub(count, step), 0);
console.log("" + back);
const from_i32 = as_usize2(12);
const from_f64 = as_usize3(7.9);
const from_u8 = as_usize(200);
const to_i32 = as_i32(count);
const to_u8 = as_u8(300);
console.log("" + from_i32 + " " + from_f64 + " " + from_u8 + " " + to_i32 + " " + to_u8);
const letters = [ "a", "b", "c", "d" ];
const at = 2;
console.log(__at(letters, at));
console.log($k(count));
console.log(JSON.stringify(count));
console.log(JSON.stringify(count));
const $o = from_json("17");
let $p = null;
if ($o[0] === 0) {
	const parsed = $o[1];
	$p = console.log("" + parsed);
} else {
	const reason = $o[1];
	$p = console.log(reason);
}
$p;
let rows = $q();
$r(rows, 3, "three");
console.log($f($s(rows, 3), "none"));
const positions = [ 0, 3, 2147483647 ];
const encoded = $y(positions);
console.log(encoded);
const decoded = $z(encoded);
const $F = decoded;
let $G = null;
if ($F[0] === 0) {
	const back2 = $F[1];
	$G = console.log("" + back2.length + " " + __at(back2, 2));
} else {
	const reason2 = $F[1];
	$G = console.log(reason2);
}
process.exit($G);
