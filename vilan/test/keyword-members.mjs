function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __json_kind(value) {
	if (value === null) return "null";
	if (Array.isArray(value)) return "array";
	return typeof value;
}
function __try_parse_json(text) {
	try {
		return [ 0, JSON.parse(text) ];
	} catch (error) {
		return [ 1 ];
	}
}
function has_field(self, name2) {
	return Object.hasOwn(self, name2);
}
function from_json_value(value) {
	let $g = null;
	if (__json_kind(value) === "string") {
		$g = [ 0, String(value) ];
	} else {
		$g = [ 1, "expected a string" ];
	}
	return $g;
}
function from_json_value2(value) {
	let $i = null;
	if (__json_kind(value) !== "number") {
		return [ 1, "expected a number" ];
	}
	$i;
	const $j = integer_lane_failure(value, true);
	let $k = null;
	if ($j[0] === 0) {
		const reason = $j[1];
		$k = [ 1, reason ];
	} else {
		$k = [ 0, Number(value) ];
	}
	return $k;
}
function from_json_value3(value) {
	let $m = null;
	if (__json_kind(value) === "boolean") {
		$m = [ 0, Boolean(value) ];
	} else {
		$m = [ 1, "expected a boolean" ];
	}
	return $m;
}
function integer_lane_failure(value, signed) {
	const number = Number(value);
	if (number !== Math.floor(number)) {
		return [ 0, "expected a whole number, found " + number ];
	}
	if (!(signed) && number < 0.0) {
		return [ 0, "expected a non-negative number, found " + number ];
	}
	return [ 1 ];
}
function type(self) {
	return "square";
}
function for2(self, times) {
	return self[0] * times;
}
function ret() {
	return [ 3 ];
}
function to_json(self) {
	return "{\"type\":" + JSON.stringify(self[0]) + "," + "\"if\":" + JSON.stringify(self[1]) + "," + "\"match\":" + JSON.stringify(self[2]) + "}";
}
function from_json(text) {
	const $c = ok_or(__try_parse_json(text), "not valid JSON");
	if ($c[0] === 1) {
		return $c;
	}
	return from_json_value4($c[1]);
}
function from_json_value4(value) {
	let $d = null;
	if (!(has_field(value, "type"))) {
		return [ 1, "missing field type" ];
	}
	$d;
	let $e = null;
	if (!(has_field(value, "if"))) {
		return [ 1, "missing field if" ];
	}
	$e;
	let $f = null;
	if (!(has_field(value, "match"))) {
		return [ 1, "missing field match" ];
	}
	$f;
	const $h = from_json_value(value["type"]);
	if ($h[0] === 1) {
		return $h;
	}
	const $l = from_json_value2(value["if"]);
	if ($l[0] === 1) {
		return $l;
	}
	const $n = from_json_value3(value["match"]);
	if ($n[0] === 1) {
		return $n;
	}
	return [ 0, [ $h[1], $l[1], $n[1] ] ];
}
function ok_or(self, err) {
	const $a = self;
	let $b = null;
	if ($a[0] === 0) {
		const x = $a[1];
		$b = [ 0, __clone(x) ];
	} else {
		$b = [ 1, __clone(err) ];
	}
	return $b;
}
function in2(self) {
	return "a " + type(self);
}
const event = [ "click", 1, true ];
console.log(event[0]);
console.log(String(event[1] + 1));
console.log(to_json(event));
const $o = from_json("{\"type\":\"key\",\"if\":2,\"match\":false}");
let $p = null;
if ($o[0] === 0) {
	const decoded = $o[1];
	$p = console.log("" + decoded[0] + " " + decoded[1] + " " + decoded[2]);
} else {
	const message = $o[1];
	$p = console.log("refused: " + message);
}
$p;
const square = ret();
console.log(type(square));
console.log(in2(square));
console.log(String(for2(square, 2)));
const found = [ 0, square ];
const $q = found;
let $r = null;
if ($q[0] === 1) {
	$r = $q;
} else {
	$r = [ 0, type($q[1]) ];
}
const kind = $r;
const $s = kind;
let $t = null;
if ($s[0] === 0) {
	const name = $s[1];
	$t = console.log(name);
} else {
	$t = console.log("none");
}
process.exit($t);
