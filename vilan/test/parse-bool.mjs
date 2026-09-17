function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __force(cell) {
	if (cell.state === 2) return cell.value;
	if (cell.state === 1) throw "lazy initialization cycle: `" + cell.name + "`";
	if (cell.state === 3) throw "lazy `" + cell.name + "` is poisoned: its initializer panicked: " + cell.value;
	cell.state = 1;
	try {
		cell.value = cell.thunk();
	} catch (failure) {
		cell.state = 3;
		cell.value = failure;
		throw failure;
	}
	cell.state = 2;
	cell.thunk = null;
	return cell.value;
}
function __lazy(name, thunk) {
	return { name: name, state: 0, value: undefined, thunk: thunk };
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
function $c(self, fallback) {
	const $d = self;
	let $e = null;
	if ($d[0] === 0) {
		const x = __clone($d[1]);
		$e = x;
	} else {
		$e = __clone(__force(fallback));
	}
	return $e;
}
function $f(self) {
	const $g = self;
	let $h = null;
	if ($g[0] === 0) {
		const x = __clone($g[1]);
		$h = x;
	} else {
		$h = (() => {
			throw "expected Some but got None";
		})();
	}
	return $h;
}
function $i(self) {
	const $j = self;
	return $j[0] === 0;
}
console.log($c(parse_bool("true"), __lazy("fallback", () => {
	return false;
})));
console.log($c(parse_bool("false"), __lazy("fallback", () => {
	return true;
})));
console.log(to_string(true));
console.log($f(parse_bool(to_string(false))));
console.log($i(parse_bool("1")));
console.log($i(parse_bool("0")));
console.log($i(parse_bool("True")));
console.log($i(parse_bool("FALSE")));
console.log($i(parse_bool("yes")));
console.log($i(parse_bool("")));
console.log($c(parse_bool(" true "), __lazy("fallback", () => {
	return false;
})));
console.log($i(parse_bool("tr ue")));
console.log($c(parse_bool("nonsense"), __lazy("fallback", () => {
	return true;
})));
