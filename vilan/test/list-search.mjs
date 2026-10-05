function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __force(cell) {
	if (cell.state === 2) return cell.value;
	if (cell.state === 1) throw "lazy initialization cycle: `" + cell.name + "`";
	if (cell.state === 3) throw "lazy `" + cell.name + "` is poisoned: its initializer panicked: " + (cell.value && cell.value.location !== undefined ? cell.value.message : cell.value);
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
function eq(self, b) {
	return self === b;
}
function eq2(self, b) {
	return self === b;
}
function max_value() {
	return 9007199254740992;
}
function find(self, predicate) {
	for (const item of self) {
		if (predicate(item)) {
			return [ 0, item ];
		}
	}
	return [ 1 ];
}
function unwrap_or(self, fallback) {
	const $a = self;
	let $b = null;
	if ($a[0] === 0) {
		const x = __clone($a[1]);
		$b = x;
	} else {
		$b = __clone(__force(fallback));
	}
	return $b;
}
function is_none(self) {
	const $c = self;
	return $c[0] === 1;
}
function contains(self, value) {
	for (const item of self) {
		if (eq2(item, value)) {
			return true;
		}
	}
	return false;
}
function index_of(self, value) {
	let index = 0;
	for (const item of self) {
		if (eq2(item, value)) {
			return [ 0, index ];
		}
		index = index + 1;
	}
	return [ 1 ];
}
function contains2(self, value) {
	for (const item of self) {
		if (eq(item, value)) {
			return true;
		}
	}
	return false;
}
function index_of2(self, value) {
	let index = 0;
	for (const item of self) {
		if (eq(item, value)) {
			return [ 0, index ];
		}
		index = index + 1;
	}
	return [ 1 ];
}
const xs = [ 10, 20, 30, 20 ];
console.log(unwrap_or(find(xs, (n) => {
	return n > 15;
}), __lazy("fallback", () => {
	return 0;
})));
console.log(is_none(find(xs, (n) => {
	return n > 90;
})));
console.log(contains(xs, 20));
console.log(contains(xs, 25));
console.log(unwrap_or(index_of(xs, 20), __lazy("fallback", () => {
	return max_value();
})));
console.log(is_none(index_of(xs, 99)));
const words = [ "alpha", "beta" ];
console.log(contains2(words, "beta"));
console.log(unwrap_or(index_of2(words, "alpha"), __lazy("fallback", () => {
	return max_value();
})));
let empty = [  ];
console.log(is_none(find(empty, (n) => {
	return n > 0;
})));
console.log(contains(empty, 1));
console.log(is_none(index_of(empty, 1)));
