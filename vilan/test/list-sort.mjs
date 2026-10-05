function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __list_sort_by(list, compare) {
	return list.slice().sort(compare);
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function compare(self, b) {
	let $c = null;
	if (self < b) {
		$c = -1;
	} else {
		let $d = null;
		if (self > b) {
			$d = 1;
		} else {
			$d = 0;
		}
		$c = $d;
	}
	return $c;
}
function compare2(self, b) {
	let $a = null;
	if (self < b) {
		$a = -1;
	} else {
		let $b = null;
		if (self > b) {
			$b = 1;
		} else {
			$b = 0;
		}
		$a = $b;
	}
	return $a;
}
function reverse(self) {
	let result = [  ];
	let index = self.length;
	while (index > 0) {
		index = index - 1;
		result.push(__clone(__at(self, index, "std/src/list.vl:83:16")));
	}
	return result;
}
function sort(self) {
	return __list_sort_by(__clone(self), (a, b) => {
		return compare2(a, b);
	});
}
function sort2(self) {
	return __list_sort_by(__clone(self), (a, b) => {
		return compare(a, b);
	});
}
const xs = [ 3, 1, 2 ];
console.log(__at(reverse(xs), 0, "list-sort.vl:11:8"));
console.log(__at(reverse(xs), 2, "list-sort.vl:12:8"));
console.log(__at(sort(xs), 0, "list-sort.vl:13:8"));
console.log(__at(sort(xs), 2, "list-sort.vl:14:8"));
console.log(__at(xs, 0, "list-sort.vl:15:8"));
const numeric = sort([ 10, 2, 1 ]);
console.log(__at(numeric, 0, "list-sort.vl:20:8"));
console.log(__at(numeric, 2, "list-sort.vl:21:8"));
const words = sort2([ "pear", "apple", "fig" ]);
console.log(__at(words, 0, "list-sort.vl:24:8"));
const descending = __list_sort_by(xs, (a, b) => {
	let $e = null;
	if (a > b) {
		$e = -1;
	} else {
		let $f = null;
		if (a < b) {
			$f = 1;
		} else {
			$f = 0;
		}
		$e = $f;
	}
	return $e;
});
console.log(__at(descending, 0, "list-sort.vl:37:8"));
let entries = [  ];
entries.push([ 1, "a" ]);
entries.push([ 0, "b" ]);
entries.push([ 1, "c" ]);
entries.push([ 0, "d" ]);
let order = "";
for (const entry of __list_sort_by(entries, (a, b) => {
	return compare2(a[0], b[0]);
})) {
	order = order + entry[1];
}
console.log(order);
