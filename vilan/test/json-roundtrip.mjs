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
	let $E = null;
	if (__json_kind(value) === "string") {
		$E = [ 0, String(value) ];
	} else {
		$E = [ 1, "expected a string" ];
	}
	return $E;
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
	return "{\"name\":" + JSON.stringify(self[0]) + "," + "\"members\":" + $l(self[1]) + "," + "\"captain\":" + $t(self[2]) + "}";
}
function from_json(text) {
	const $A = $x(__try_parse_json(text), "not valid JSON");
	if ($A[0] === 1) {
		return $A;
	}
	return from_json_value3($A[1]);
}
function from_json_value3(value) {
	let $B = null;
	if (!(has_field(value, "name"))) {
		return [ 1, "missing field name" ];
	}
	$B;
	let $C = null;
	if (!(has_field(value, "members"))) {
		return [ 1, "missing field members" ];
	}
	$C;
	let $D = null;
	if (!(has_field(value, "captain"))) {
		return [ 1, "missing field captain" ];
	}
	$D;
	const $F = from_json_value(value["name"]);
	if ($F[0] === 1) {
		return $F;
	}
	const $K = $G(value["members"]);
	if ($K[0] === 1) {
		return $K;
	}
	const $O = $L(value["captain"]);
	if ($O[0] === 1) {
		return $O;
	}
	return [ 0, [ $F[1], $K[1], $O[1] ] ];
}
function $d(value) {
	let $e = null;
	if (__json_kind(value) !== "array") {
		return [ 1, "expected an array" ];
	}
	$e;
	let result = [  ];
	for (const element of value) {
		const $j = result;
		const $i = from_json_value2(element);
		if ($i[0] === 1) {
			return $i;
		}
		$j.push($i[1]);
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
function $l(self) {
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
function $p(value) {
	let $q = null;
	if (value === null) {
		$q = [ 0, [ 1 ] ];
	} else {
		const $r = from_json_value2(value);
		if ($r[0] === 1) {
			return $r;
		}
		$q = [ 0, [ 0, $r[1] ] ];
	}
	return $q;
}
function $m(text) {
	const $n = __try_parse_json(text);
	let $o = null;
	if ($n[0] === 0) {
		const value = $n[1];
		$o = $p(value);
	} else {
		$o = [ 1, "not valid JSON" ];
	}
	return $o;
}
function $t(self) {
	const $u = self;
	let $v = null;
	if ($u[0] === 0) {
		const value = $u[1];
		$v = JSON.stringify(value);
	} else {
		$v = "null";
	}
	return $v;
}
function $x(self, err) {
	const $y = self;
	let $z = null;
	if ($y[0] === 0) {
		const x = $y[1];
		$z = [ 0, __clone(x) ];
	} else {
		$z = [ 1, __clone(err) ];
	}
	return $z;
}
function $G(value) {
	let $H = null;
	if (__json_kind(value) !== "array") {
		return [ 1, "expected an array" ];
	}
	$H;
	let result = [  ];
	for (const element of value) {
		const $J = result;
		const $I = from_json_value(element);
		if ($I[0] === 1) {
			return $I;
		}
		$J.push($I[1]);
	}
	return [ 0, result ];
}
function $L(value) {
	let $M = null;
	if (value === null) {
		$M = [ 0, [ 1 ] ];
	} else {
		const $N = from_json_value(value);
		if ($N[0] === 1) {
			return $N;
		}
		$M = [ 0, [ 0, $N[1] ] ];
	}
	return $M;
}
function $X(value) {
	let $Y = null;
	if (__json_kind(value) !== "array") {
		return [ 1, "expected an array" ];
	}
	$Y;
	let result = [  ];
	for (const element of value) {
		const $aa = result;
		const $Z = from_json_value3(element);
		if ($Z[0] === 1) {
			return $Z;
		}
		$aa.push($Z[1]);
	}
	return [ 0, result ];
}
function $U(text) {
	const $V = __try_parse_json(text);
	let $W = null;
	if ($V[0] === 0) {
		const value = $V[1];
		$W = $X(value);
	} else {
		$W = [ 1, "not valid JSON" ];
	}
	return $W;
}
function $ac(self) {
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
const $k = nums;
console.log($k[0] === 0 && $l($k[1]) === "[1,2,3]");
const some = $m("7");
const $s = some;
console.log($s[0] === 0 && $t($s[1]) === "7");
const none = $m("null");
const $w = none;
console.log($w[0] === 0 && $t($w[1]) === "null");
const json = "{\"name\":\"Reds\",\"members\":[\"Ada\",\"Bob\"],\"captain\":\"Ada\"}";
const $P = from_json(json);
console.log($P[0] === 0 && to_json($P[1]) === json && $l($P[1][1]) === "[\"Ada\",\"Bob\"]");
const teams = $U("[" + json + "]");
const $ab = teams;
console.log($ab[0] === 0 && $ac($ab[1]) === "[" + json + "]");
