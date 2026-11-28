function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __hash(value) {
	return (typeof value === "object" && value !== null) ? JSON.stringify(value) : value;
}
function __key_at(value, key) {
	const parts = key[0][0];
	for (let at = 0; at < parts.length; at++) value = value[parts[at][1]];
	return value;
}
function __key_put(value, key, field) {
	const parts = key[0][0];
	const last = parts.length - 1;
	for (let at = 0; at < last; at++) value = value[parts[at][1]];
	return value[parts[last][1]] = field;
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function __shared_new(value) {
	return { v: value };
}
function eq(self, b) {
	return self === b;
}
function assert(condition, message, caller) {
	let $f = null;
	if (!(condition)) {
		$f = (() => {
			throw __panic(message, caller);
		})();
	}
	return $f;
}
function name(self) {
	let name3 = "";
	for (const step of self[0]) {
		name3 = name3 + spelled(step, name3 === "");
	}
	return name3;
}
function then(self, next) {
	let parts = [  ];
	for (const step of self[0]) {
		parts.push(__clone(step));
	}
	for (const step2 of next[0]) {
		parts.push(__clone(step2));
	}
	return [ parts ];
}
function eq2(self, other) {
	return eq(__hash(self[0]), __hash(other[0]));
}
function spelled(step, leads) {
	const $g = step[3];
	if ($g[0] === 1) {
		let $h = null;
		if ($g[2] > 1) {
			$h = "::" + step[2] + "." + $g[1];
		} else {
			$h = "::" + step[2];
		}
		return $h;
	}
	let $i = null;
	if (leads) {
		$i = step[2];
	} else {
		$i = "." + step[2];
	}
	return $i;
}
function city_of(user2) {
	return get(at2(at(user2, $b), $c));
}
const $a = [ [ [ [ "field-syntax-keys.vl::User", 0, "name", [ 0 ] ] ] ] ];
function at(self, key) {
	return [ self[0], __clone(key) ];
}
function get(self) {
	return __key_at(self[0].v, self[1]);
}
const $b = [ [ [ [ "field-syntax-keys.vl::User", 1, "address", [ 0 ] ] ] ] ];
const $c = [ [ [ [ "field-syntax-keys.vl::Address", 0, "city", [ 0 ] ] ] ] ];
function then2(self, next) {
	return [ then(self[0], next[0]) ];
}
function at2(self, key) {
	return [ self[0], then2(self[1], key) ];
}
function set(self, value) {
	__key_put(self[0].v, self[1], __clone(value));
}
const $d = [ [ [ [ "field-syntax-keys.vl::Address", 1, "zip", [ 0 ] ] ] ] ];
function at3(self, key) {
	return [ self[0], then2(self[1], key) ];
}
const $e = [ [ [ [ "field-syntax-keys.vl::User", 2, "get", [ 0 ] ] ] ] ];
function eq3(self, other) {
	return eq2(self[0], other[0]);
}
function name2(self) {
	return name(self[0]);
}
const $j = [ [ [ [ "field-syntax-keys.vl::Shelf", 0, "pair", [ 0 ] ] ] ] ];
const $k = [ [ [ [ "field-syntax-keys.vl::Pair", 1, "right", [ 0 ] ] ] ] ];
const $l = [ [ [ [ "field-syntax-keys.vl::Pair", 0, "left", [ 0 ] ] ] ] ];
const user = [ __shared_new([ "ada", [ "london", 1 ], "verb" ]) ];
console.log(get(at(user, $a)));
const city = at2(at(user, $b), $c);
set(city, "paris");
console.log(get(at2(at(user, $b), $c)));
console.log(city_of(user));
const zip = at3(at(user, $b), $d);
set(zip, get(zip) + 41);
console.log(String(user[0].v[1][1]));
console.log(get(at(user, $e)));
assert(eq3(at2(at(user, $b), $c)[1], at2(at(user, $b), $c)[1]), "one key", "field-syntax-keys.vl:88:2");
console.log(name2(at2(at(user, $b), $c)[1]));
const shelf = [ __shared_new([ [ "l", "r" ] ]) ];
const right = at3(at(shelf, $j), $k);
set(right, "R");
const left = get(at3(at(shelf, $j), $l));
console.log(left + get(right));
//# sourceMappingURL=field-syntax-keys.mjs.map
