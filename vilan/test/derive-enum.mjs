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
function __json_tag(value) {
	if (typeof value === "string") return value;
	if (value === null || typeof value !== "object" || Array.isArray(value)) return "";
	return Object.keys(value)[0] ?? "";
}
function __try_parse_json(text) {
	try {
		return [ 0, JSON.parse(text) ];
	} catch (error) {
		return [ 1 ];
	}
}
function from_json_value(value) {
	let $l = null;
	if (__json_kind(value) !== "number") {
		return [ 1, "expected a number" ];
	}
	$l;
	const $m = integer_lane_failure(value, true);
	let $n = null;
	if ($m[0] === 0) {
		const reason = $m[1];
		$n = [ 1, reason ];
	} else {
		$n = [ 0, Number(value) ];
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
function eq(self, other) {
	const $a = [ self, other ];
	let $b = null;
	if ($a[0][0] === 0 && $a[1][0] === 0) {
		const s0 = $a[0][1];
		const o0 = $a[1][1];
		$b = s0 === o0;
	} else if ($a[0][0] === 1 && $a[1][0] === 1) {
		const s02 = $a[0][1];
		const s1 = $a[0][2];
		const o02 = $a[1][1];
		const o1 = $a[1][2];
		$b = s02 === o02 && s1 === o1;
	} else if ($a[0][0] === 2 && $a[1][0] === 2) {
		$b = true;
	} else {
		$b = false;
	}
	return $b;
}
function debug(self) {
	const $c = self;
	let $d = null;
	if ($c[0] === 0) {
		const p0 = $c[1];
		$d = "Circle(" + JSON.stringify(p0) + ")";
	} else if ($c[0] === 1) {
		const p02 = $c[1];
		const p1 = $c[2];
		$d = "Rect(" + JSON.stringify(p02) + ", " + JSON.stringify(p1) + ")";
	} else {
		$d = "Empty";
	}
	return $d;
}
function to_json(self) {
	const $e = self;
	let $f = null;
	if ($e[0] === 0) {
		const p0 = $e[1];
		$f = "{\"Circle\":" + JSON.stringify(p0) + "}";
	} else if ($e[0] === 1) {
		const p02 = $e[1];
		const p1 = $e[2];
		$f = "{\"Rect\":[" + JSON.stringify(p02) + "," + JSON.stringify(p1) + "]}";
	} else {
		$f = "\"Empty\"";
	}
	return $f;
}
function from_json(text) {
	const $i = ok_or(__try_parse_json(text), "not valid JSON");
	if ($i[0] === 1) {
		return $i;
	}
	return from_json_value2($i[1]);
}
function from_json_value2(value) {
	const $j = __json_tag(value);
	let $k = null;
	if ($j === "Circle") {
		const $o = from_json_value(value["Circle"]);
		if ($o[0] === 1) {
			return $o;
		}
		$k = [ 0, [ 0, $o[1] ] ];
	} else if ($j === "Rect") {
		const $p = from_json_value(value["Rect"]["0"]);
		if ($p[0] === 1) {
			return $p;
		}
		const $q = from_json_value(value["Rect"]["1"]);
		if ($q[0] === 1) {
			return $q;
		}
		$k = [ 0, [ 1, $p[1], $q[1] ] ];
	} else if ($j === "Empty") {
		$k = [ 0, [ 2 ] ];
	} else {
		$k = [ 1, "unknown variant in JSON for enum Shape" ];
	}
	return $k;
}
function ok_or(self, err) {
	const $g = self;
	let $h = null;
	if ($g[0] === 0) {
		const x = $g[1];
		$h = [ 0, __clone(x) ];
	} else {
		$h = [ 1, __clone(err) ];
	}
	return $h;
}
const c = [ 0, 3 ];
const r = [ 1, 4, 5 ];
const e = [ 2 ];
console.log(eq(c, [ 0, 3 ]));
console.log(eq(c, [ 0, 9 ]));
console.log(eq(c, r));
console.log(eq(e, [ 2 ]));
console.log(debug(c));
console.log(debug(r));
console.log(debug(e));
console.log(to_json(c));
console.log(to_json(r));
console.log(to_json(e));
const $r = from_json(to_json(c));
console.log($r[0] === 0 && eq($r[1], c));
const $s = from_json(to_json(r));
console.log($s[0] === 0 && eq($s[1], r));
const $t = from_json(to_json(e));
console.log($t[0] === 0 && eq($t[1], e));
const $u = from_json("\"Hexagon\"");
if ($u[0] === 1) {
	console.log(true);
	console.log($u[1]);
}
