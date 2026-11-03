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
function __hash(value) {
	return (typeof value === "object" && value !== null) ? JSON.stringify(value) : value;
}
function __json_kind(value) {
	if (value === null) return "null";
	if (Array.isArray(value)) return "array";
	return typeof value;
}
function __nmap_get(table, hash) {
	const entry = table[0].get(hash);
	return entry !== undefined && entry[1][0] === 0 ? [ 0, __clone(entry[1][1]) ] : [ 1 ];
}
function __nmap_insert(table, hash, key, value) {
	__nmap_settle(table);
	const entry = table[0].get(hash);
	if (entry === undefined) table[0].set(hash, [ key, [ 0, value ] ]);
	else {
		entry[0] = key;
		if (entry[1][0] !== 0 || entry[1][1] !== value) entry[1] = [ 0, value ];
	}
}
function __nmap_settle(table) {
	const hash = table[1];
	if (hash === undefined) return;
	table[1] = undefined;
	const entry = table[0].get(hash);
	if (entry !== undefined && entry[1][0] === 1) table[0].delete(hash);
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
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
	const $i = __try_parse_json(text);
	let $j = null;
	if ($i[0] === 0) {
		const value = $i[1];
		$j = from_json_value(value);
	} else {
		$j = [ 1, "not valid JSON" ];
	}
	return $j;
}
function from_json_value(value) {
	let $k = null;
	if (__json_kind(value) === "number") {
		$k = [ 0, Number(value) ];
	} else {
		$k = [ 1, "expected a number" ];
	}
	return $k;
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $g = null;
	if (wrapped < 0) {
		$g = wrapped + modulus;
	} else {
		$g = wrapped;
	}
	return $g;
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
	let $h = null;
	if (wrapped >= half) {
		$h = wrapped - modulus;
	} else {
		$h = wrapped;
	}
	return $h;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(Math.trunc(widened));
}
function as_usize2(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
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
	let $b = null;
	if (self >= other) {
		$b = [ 0, self - other ];
	} else {
		$b = [ 1 ];
	}
	return $b;
}
function saturating_sub(self, other) {
	let $a = null;
	if (self >= other) {
		$a = self - other;
	} else {
		$a = 0;
	}
	return $a;
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
	return Number(saturate_unsigned(widened));
}
function div(self, b) {
	return Math.trunc(self / b);
}
function halve(value, divisor) {
	return div(value, divisor);
}
function is_none(self) {
	const $c = self;
	return $c[0] === 1;
}
function unwrap_or(self, fallback) {
	const $d = self;
	let $e = null;
	if ($d[0] === 0) {
		const x = __clone($d[1]);
		$e = x;
	} else {
		$e = __clone(fallback);
	}
	return $e;
}
function format(value) {
	return to_string(value);
}
function new2() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function insert(self, key, value) {
	__nmap_insert(self[0], hash(key), __clone(key), __clone(value));
}
function get(self, key) {
	return __nmap_get(self[0], hash(key));
}
function to_json(self) {
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
function from_json_value2(value) {
	let $r = null;
	if (__json_kind(value) !== "array") {
		return [ 1, "expected an array" ];
	}
	$r;
	let result = [  ];
	for (const element of value) {
		const $t = result;
		const $s = from_json_value(element);
		if ($s[0] === 1) {
			return $s;
		}
		$t.push($s[1]);
	}
	return [ 0, result ];
}
function from_json2(text) {
	const $p = __try_parse_json(text);
	let $q = null;
	if ($p[0] === 0) {
		const value = $p[1];
		$q = from_json_value2(value);
	} else {
		$q = [ 1, "not valid JSON" ];
	}
	return $q;
}
const count = 42;
const step = 5;
const total = count + step;
const left = count - step;
const scaled = count * step;
const quotient = Math.trunc(count / step);
const remainder = rem(count, step);
console.log("" + total + " " + left + " " + scaled + " " + quotient + " " + remainder);
const halved = halve(count, 4);
console.log("" + halved);
console.log("" + (count > step) + " " + (count === 42) + " " + (count !== step));
const top = max_value();
const bottom = min_value();
console.log("" + top + " " + bottom);
const floor = saturating_sub(step, count);
console.log("" + floor);
console.log(is_none(checked_sub(step, count)));
const back = unwrap_or(checked_sub(count, step), 0);
console.log("" + back);
const from_i32 = as_usize2(12);
const from_f64 = as_usize3(7.9);
const from_u8 = as_usize(200);
const to_i32 = as_i32(count);
const to_u8 = as_u8(300);
console.log("" + from_i32 + " " + from_f64 + " " + from_u8 + " " + to_i32 + " " + to_u8);
const letters = [ "a", "b", "c", "d" ];
const at = 2;
console.log(__at(letters, at, "usize.vl:55:8"));
console.log(format(count));
console.log(JSON.stringify(count));
console.log(JSON.stringify(count));
const $l = from_json("17");
let $m = null;
if ($l[0] === 0) {
	const parsed = $l[1];
	$m = console.log("" + parsed);
} else {
	const reason = $l[1];
	$m = console.log(reason);
}
$m;
let rows = new2();
insert(rows, 3, "three");
console.log(unwrap_or(get(rows, 3), "none"));
const positions = [ 0, 3, 2147483647 ];
const encoded = to_json(positions);
console.log(encoded);
const decoded = from_json2(encoded);
const $u = decoded;
let $v = null;
if ($u[0] === 0) {
	const back2 = $u[1];
	$v = console.log("" + back2.length + " " + __at(back2, 2, "usize.vl:75:41"));
} else {
	const reason2 = $u[1];
	$v = console.log(reason2);
}
$v;
//# sourceMappingURL=usize.mjs.map
