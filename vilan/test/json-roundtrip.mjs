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
	let $x = null;
	if (__json_kind(value) === "string") {
		$x = [ 0, String(value) ];
	} else {
		$x = [ 1, "expected a string" ];
	}
	return $x;
}
function from_json_value2(value) {
	let $d = null;
	if (__json_kind(value) !== "number") {
		return [ 1, "expected a number" ];
	}
	$d;
	const $e = integer_lane_failure(value, true);
	let $f = null;
	if ($e[0] === 0) {
		const reason = $e[1];
		$f = [ 1, reason ];
	} else {
		$f = [ 0, Number(value) ];
	}
	return $f;
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
	return "{\"name\":" + JSON.stringify(self[0]) + "," + "\"members\":" + to_json2(self[1]) + "," + "\"captain\":" + to_json3(self[2]) + "}";
}
function from_json(text) {
	const $t = ok_or(__try_parse_json(text), "not valid JSON");
	if ($t[0] === 1) {
		return $t;
	}
	return from_json_value3($t[1]);
}
function from_json_value3(value) {
	let $u = null;
	if (!(has_field(value, "name"))) {
		return [ 1, "missing field name" ];
	}
	$u;
	let $v = null;
	if (!(has_field(value, "members"))) {
		return [ 1, "missing field members" ];
	}
	$v;
	let $w = null;
	if (!(has_field(value, "captain"))) {
		return [ 1, "missing field captain" ];
	}
	$w;
	const $y = from_json_value(value["name"]);
	if ($y[0] === 1) {
		return $y;
	}
	const $C = from_json_value6(value["members"]);
	if ($C[0] === 1) {
		return $C;
	}
	const $F = from_json_value7(value["captain"]);
	if ($F[0] === 1) {
		return $F;
	}
	return [ 0, [ $y[1], $C[1], $F[1] ] ];
}
function from_json_value4(value) {
	let $c = null;
	if (__json_kind(value) !== "array") {
		return [ 1, "expected an array" ];
	}
	$c;
	let result = [  ];
	for (const element of value) {
		const $h = result;
		const $g = from_json_value2(element);
		if ($g[0] === 1) {
			return $g;
		}
		$h.push($g[1]);
	}
	return [ 0, result ];
}
function from_json2(text) {
	const $a = __try_parse_json(text);
	let $b = null;
	if ($a[0] === 0) {
		const value = $a[1];
		$b = from_json_value4(value);
	} else {
		$b = [ 1, "not valid JSON" ];
	}
	return $b;
}
function to_json2(self) {
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
function from_json_value5(value) {
	let $l = null;
	if (value === null) {
		$l = [ 0, [ 1 ] ];
	} else {
		const $m = from_json_value2(value);
		if ($m[0] === 1) {
			return $m;
		}
		$l = [ 0, [ 0, $m[1] ] ];
	}
	return $l;
}
function from_json3(text) {
	const $j = __try_parse_json(text);
	let $k = null;
	if ($j[0] === 0) {
		const value = $j[1];
		$k = from_json_value5(value);
	} else {
		$k = [ 1, "not valid JSON" ];
	}
	return $k;
}
function to_json3(self) {
	const $o = self;
	let $p = null;
	if ($o[0] === 0) {
		const value = $o[1];
		$p = JSON.stringify(value);
	} else {
		$p = "null";
	}
	return $p;
}
function ok_or(self, err) {
	const $r = self;
	let $s = null;
	if ($r[0] === 0) {
		const x = $r[1];
		$s = [ 0, __clone(x) ];
	} else {
		$s = [ 1, __clone(err) ];
	}
	return $s;
}
function from_json_value6(value) {
	let $z = null;
	if (__json_kind(value) !== "array") {
		return [ 1, "expected an array" ];
	}
	$z;
	let result = [  ];
	for (const element of value) {
		const $B = result;
		const $A = from_json_value(element);
		if ($A[0] === 1) {
			return $A;
		}
		$B.push($A[1]);
	}
	return [ 0, result ];
}
function from_json_value7(value) {
	let $D = null;
	if (value === null) {
		$D = [ 0, [ 1 ] ];
	} else {
		const $E = from_json_value(value);
		if ($E[0] === 1) {
			return $E;
		}
		$D = [ 0, [ 0, $E[1] ] ];
	}
	return $D;
}
function from_json_value8(value) {
	let $L = null;
	if (__json_kind(value) !== "array") {
		return [ 1, "expected an array" ];
	}
	$L;
	let result = [  ];
	for (const element of value) {
		const $N = result;
		const $M = from_json_value3(element);
		if ($M[0] === 1) {
			return $M;
		}
		$N.push($M[1]);
	}
	return [ 0, result ];
}
function from_json4(text) {
	const $J = __try_parse_json(text);
	let $K = null;
	if ($J[0] === 0) {
		const value = $J[1];
		$K = from_json_value8(value);
	} else {
		$K = [ 1, "not valid JSON" ];
	}
	return $K;
}
function to_json6(self) {
	let result = "[";
	let first = true;
	for (const element of self) {
		if (!(first)) {
			result = result + ",";
		}
		result = result + to_json(element);
		first = false;
	}
	return result + "]";
}
const nums = from_json2("[1,2,3]");
const $i = nums;
console.log($i[0] === 0 && to_json2($i[1]) === "[1,2,3]");
const some = from_json3("7");
const $n = some;
console.log($n[0] === 0 && to_json3($n[1]) === "7");
const none = from_json3("null");
const $q = none;
console.log($q[0] === 0 && to_json3($q[1]) === "null");
const json = "{\"name\":\"Reds\",\"members\":[\"Ada\",\"Bob\"],\"captain\":\"Ada\"}";
const $G = from_json(json);
console.log($G[0] === 0 && to_json($G[1]) === json && to_json2($G[1][1]) === "[\"Ada\",\"Bob\"]");
const teams = from_json4("[" + json + "]");
const $O = teams;
console.log($O[0] === 0 && to_json6($O[1]) === "[" + json + "]");
