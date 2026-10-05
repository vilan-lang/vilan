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
function has_field(self, name) {
	return Object.hasOwn(self, name);
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
function to_json(self) {
	return "{\"x\":" + JSON.stringify(self[0]) + "," + "\"y\":" + JSON.stringify(self[1]) + "}";
}
function from_json_value4(value) {
	let $p = null;
	if (!(has_field(value, "x"))) {
		return [ 1, "missing field x" ];
	}
	$p;
	let $q = null;
	if (!(has_field(value, "y"))) {
		return [ 1, "missing field y" ];
	}
	$q;
	const $r = from_json_value2(value["x"]);
	if ($r[0] === 1) {
		return $r;
	}
	const $s = from_json_value2(value["y"]);
	if ($s[0] === 1) {
		return $s;
	}
	return [ 0, [ $r[1], $s[1] ] ];
}
function to_json2(self) {
	return "{\"name\":" + JSON.stringify(self[0]) + "," + "\"age\":" + JSON.stringify(self[1]) + "," + "\"active\":" + JSON.stringify(self[2]) + "," + "\"home\":" + to_json(self[3]) + "}";
}
function from_json(text2) {
	const $c = ok_or(__try_parse_json(text2), "not valid JSON");
	if ($c[0] === 1) {
		return $c;
	}
	return from_json_value5($c[1]);
}
function from_json_value5(value) {
	let $d = null;
	if (!(has_field(value, "name"))) {
		return [ 1, "missing field name" ];
	}
	$d;
	let $e = null;
	if (!(has_field(value, "age"))) {
		return [ 1, "missing field age" ];
	}
	$e;
	let $f = null;
	if (!(has_field(value, "active"))) {
		return [ 1, "missing field active" ];
	}
	$f;
	let $g = null;
	if (!(has_field(value, "home"))) {
		return [ 1, "missing field home" ];
	}
	$g;
	const $i = from_json_value(value["name"]);
	if ($i[0] === 1) {
		return $i;
	}
	const $m = from_json_value2(value["age"]);
	if ($m[0] === 1) {
		return $m;
	}
	const $o = from_json_value3(value["active"]);
	if ($o[0] === 1) {
		return $o;
	}
	const $t = from_json_value4(value["home"]);
	if ($t[0] === 1) {
		return $t;
	}
	return [ 0, [ $i[1], $m[1], $o[1], $t[1] ] ];
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
const p = [ 1, 2 ];
console.log(to_json(p));
const person = [ "Ada \"A\"", 36, true, [ 3, 4 ] ];
const text = to_json2(person);
console.log(text);
const $u = from_json(text);
console.log($u[0] === 0 && to_json2($u[1]) === text && $u[1][3][1] === 4 && $u[1][0] === "Ada \"A\"");
const $v = from_json("{\"name\":\"x\",\"age\":1,\"active\":true}");
if ($v[0] === 1) {
	console.log(true);
	console.log($v[1]);
}
const $w = from_json("{\"name\":5,\"age\":1,\"active\":true,\"home\":{\"x\":0,\"y\":0}}");
if ($w[0] === 1) {
	console.log(true);
	console.log($w[1]);
}
