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
	let $h = null;
	if (__json_kind(value) === "string") {
		$h = [ 0, String(value) ];
	} else {
		$h = [ 1, "expected a string" ];
	}
	return $h;
}
function from_json_value2(value) {
	let $j = null;
	if (__json_kind(value) !== "number") {
		return [ 1, "expected a number" ];
	}
	$j;
	const $k = integer_lane_failure(value, true);
	let $l = null;
	if ($k[0] === 0) {
		const reason = $k[1];
		$l = [ 1, reason ];
	} else {
		$l = [ 0, Number(value) ];
	}
	return $l;
}
function from_json_value3(value) {
	let $n = null;
	if (__json_kind(value) === "boolean") {
		$n = [ 0, Boolean(value) ];
	} else {
		$n = [ 1, "expected a boolean" ];
	}
	return $n;
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
	const $d = $a(__try_parse_json(text), "not valid JSON");
	if ($d[0] === 1) {
		return $d;
	}
	return from_json_value4($d[1]);
}
function from_json_value4(value) {
	let $e = null;
	if (!(has_field(value, "type"))) {
		return [ 1, "missing field type" ];
	}
	$e;
	let $f = null;
	if (!(has_field(value, "if"))) {
		return [ 1, "missing field if" ];
	}
	$f;
	let $g = null;
	if (!(has_field(value, "match"))) {
		return [ 1, "missing field match" ];
	}
	$g;
	const $i = from_json_value(value["type"]);
	if ($i[0] === 1) {
		return $i;
	}
	const $m = from_json_value2(value["if"]);
	if ($m[0] === 1) {
		return $m;
	}
	const $o = from_json_value3(value["match"]);
	if ($o[0] === 1) {
		return $o;
	}
	return [ 0, [ $i[1], $m[1], $o[1] ] ];
}
function $a(self, err) {
	const $b = self;
	let $c = null;
	if ($b[0] === 0) {
		const x = $b[1];
		$c = [ 0, __clone(x) ];
	} else {
		$c = [ 1, __clone(err) ];
	}
	return $c;
}
function $r(self) {
	return "a " + type(self);
}
const event = [ "click", 1, true ];
console.log(event[0]);
console.log(event[1] + 1);
console.log(to_json(event));
const $p = from_json("{\"type\":\"key\",\"if\":2,\"match\":false}");
let $q = null;
if ($p[0] === 0) {
	const decoded = $p[1];
	$q = console.log("" + decoded[0] + " " + decoded[1] + " " + decoded[2]);
} else {
	const message = $p[1];
	$q = console.log("refused: " + message);
}
$q;
const square = ret();
console.log(type(square));
console.log($r(square));
console.log(for2(square, 2));
const found = [ 0, square ];
const $s = found;
let $t = null;
if ($s[0] === 1) {
	$t = $s;
} else {
	$t = [ 0, type($s[1]) ];
}
const kind = $t;
const $u = kind;
let $v = null;
if ($u[0] === 0) {
	const name = $u[1];
	$v = console.log(name);
} else {
	$v = console.log("none");
}
process.exit($v);
