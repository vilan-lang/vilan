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
	let $D = null;
	if (__json_kind(value) === "string") {
		$D = [ 0, String(value) ];
	} else {
		$D = [ 1, "expected a string" ];
	}
	return $D;
}
function from_json_value2(value) {
	let $f = null;
	if (__json_kind(value) !== "number") {
		return [ 1, "expected a number" ];
	}
	$f;
	const $g = integer_lane_failure(value, true);
	let $h = null;
	if ($g[0] === 0) {
		const reason = $g[1];
		$h = [ 1, reason ];
	} else {
		$h = [ 0, Number(value) ];
	}
	return $h;
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
	return "{\"name\":" + JSON.stringify(self[0]) + "," + "\"members\":" + $k(self[1]) + "," + "\"captain\":" + $s(self[2]) + "}";
}
function from_json(text) {
	const $z = $w(__try_parse_json(text), "not valid JSON");
	if ($z[0] === 1) {
		return $z;
	}
	return from_json_value3($z[1]);
}
function from_json_value3(value) {
	let $A = null;
	if (!(has_field(value, "name"))) {
		return [ 1, "missing field name" ];
	}
	$A;
	let $B = null;
	if (!(has_field(value, "members"))) {
		return [ 1, "missing field members" ];
	}
	$B;
	let $C = null;
	if (!(has_field(value, "captain"))) {
		return [ 1, "missing field captain" ];
	}
	$C;
	const $E = from_json_value(value["name"]);
	if ($E[0] === 1) {
		return $E;
	}
	const $I = $F(value["members"]);
	if ($I[0] === 1) {
		return $I;
	}
	const $M = $J(value["captain"]);
	if ($M[0] === 1) {
		return $M;
	}
	return [ 0, [ $E[1], $I[1], $M[1] ] ];
}
function $d(value) {
	let $e = null;
	if (__json_kind(value) !== "array") {
		return [ 1, "expected an array" ];
	}
	$e;
	let result = [  ];
	for (const element of value) {
		const $i = from_json_value2(element);
		if ($i[0] === 1) {
			return $i;
		}
		result.push($i[1]);
	}
	return [ 0, result ];
}
function $a(text) {
	const $b = __try_parse_json(text);
	let $c = null;
	if ($b[0] === 0) {
		const value = $b[1];
		$c = $d(value);
	} else {
		$c = [ 1, "not valid JSON" ];
	}
	return $c;
}
function $k(self) {
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
function $o(value) {
	let $p = null;
	if (value === null) {
		$p = [ 0, [ 1 ] ];
	} else {
		const $q = from_json_value2(value);
		if ($q[0] === 1) {
			return $q;
		}
		$p = [ 0, [ 0, $q[1] ] ];
	}
	return $p;
}
function $l(text) {
	const $m = __try_parse_json(text);
	let $n = null;
	if ($m[0] === 0) {
		const value = $m[1];
		$n = $o(value);
	} else {
		$n = [ 1, "not valid JSON" ];
	}
	return $n;
}
function $s(self) {
	const $t = self;
	let $u = null;
	if ($t[0] === 0) {
		const value = $t[1];
		$u = JSON.stringify(value);
	} else {
		$u = "null";
	}
	return $u;
}
function $w(self, err) {
	const $x = self;
	let $y = null;
	if ($x[0] === 0) {
		const x = $x[1];
		$y = [ 0, __clone(x) ];
	} else {
		$y = [ 1, __clone(err) ];
	}
	return $y;
}
function $F(value) {
	let $G = null;
	if (__json_kind(value) !== "array") {
		return [ 1, "expected an array" ];
	}
	$G;
	let result = [  ];
	for (const element of value) {
		const $H = from_json_value(element);
		if ($H[0] === 1) {
			return $H;
		}
		result.push($H[1]);
	}
	return [ 0, result ];
}
function $J(value) {
	let $K = null;
	if (value === null) {
		$K = [ 0, [ 1 ] ];
	} else {
		const $L = from_json_value(value);
		if ($L[0] === 1) {
			return $L;
		}
		$K = [ 0, [ 0, $L[1] ] ];
	}
	return $K;
}
function $V(value) {
	let $W = null;
	if (__json_kind(value) !== "array") {
		return [ 1, "expected an array" ];
	}
	$W;
	let result = [  ];
	for (const element of value) {
		const $X = from_json_value3(element);
		if ($X[0] === 1) {
			return $X;
		}
		result.push($X[1]);
	}
	return [ 0, result ];
}
function $S(text) {
	const $T = __try_parse_json(text);
	let $U = null;
	if ($T[0] === 0) {
		const value = $T[1];
		$U = $V(value);
	} else {
		$U = [ 1, "not valid JSON" ];
	}
	return $U;
}
function $Z(self) {
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
const nums = $a("[1,2,3]");
const $j = nums;
console.log($j[0] === 0 && $k($j[1]) === "[1,2,3]");
const some = $l("7");
const $r = some;
console.log($r[0] === 0 && $s($r[1]) === "7");
const none = $l("null");
const $v = none;
console.log($v[0] === 0 && $s($v[1]) === "null");
const json = "{\"name\":\"Reds\",\"members\":[\"Ada\",\"Bob\"],\"captain\":\"Ada\"}";
const $N = from_json(json);
console.log($N[0] === 0 && to_json($N[1]) === json && $k($N[1][1]) === "[\"Ada\",\"Bob\"]");
const teams = $S("[" + json + "]");
const $Y = teams;
console.log($Y[0] === 0 && $Z($Y[1]) === "[" + json + "]");
