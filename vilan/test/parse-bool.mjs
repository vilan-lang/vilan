function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function to_string(self) {
	return "" + self;
}
function parse_bool(self) {
	const $a = self.trim();
	let $b = null;
	if ($a === "true") {
		$b = [ 0, true ];
	} else if ($a === "false") {
		$b = [ 0, false ];
	} else {
		$b = [ 1 ];
	}
	return $b;
}
function unwrap_or(self, fallback) {
	const $c = self;
	let $d = null;
	if ($c[0] === 0) {
		const x = __clone($c[1]);
		$d = x;
	} else {
		$d = __clone(fallback);
	}
	return $d;
}
function unwrap(self, caller) {
	const $e = self;
	let $f = null;
	if ($e[0] === 0) {
		const x = __clone($e[1]);
		$f = x;
	} else {
		$f = (() => {
			throw __panic("expected Some but got None", caller);
		})();
	}
	return $f;
}
function is_some(self) {
	const $g = self;
	return $g[0] === 0;
}
console.log(unwrap_or(parse_bool("true"), false));
console.log(unwrap_or(parse_bool("false"), true));
console.log(to_string(true));
console.log(unwrap(parse_bool(to_string(false)), "parse-bool.vl:12:39"));
console.log(is_some(parse_bool("1")));
console.log(is_some(parse_bool("0")));
console.log(is_some(parse_bool("True")));
console.log(is_some(parse_bool("FALSE")));
console.log(is_some(parse_bool("yes")));
console.log(is_some(parse_bool("")));
console.log(unwrap_or(parse_bool(" true "), false));
console.log(is_some(parse_bool("tr ue")));
console.log(unwrap_or(parse_bool("nonsense"), true));
