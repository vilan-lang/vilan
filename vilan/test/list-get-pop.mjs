function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __list_get(list, index) {
	return index >= 0 && index < list.length ? [ 0, __clone(list[index]) ] : [ 1 ];
}
function __list_pop(list) {
	return list.length === 0 ? [ 1 ] : [ 0, list.pop() ];
}
function unwrap_or(self, fallback) {
	const $a = self;
	let $b = null;
	if ($a[0] === 0) {
		const x = __clone($a[1]);
		$b = x;
	} else {
		$b = __clone(fallback);
	}
	return $b;
}
function is_none(self) {
	const $c = self;
	return $c[0] === 1;
}
function first(self) {
	return __list_get(self, 0);
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $d = null;
	if (is_empty(self)) {
		$d = [ 1 ];
	} else {
		$d = __list_get(self, self.length - 1);
	}
	return $d;
}
let xs = [  ];
xs.push(10);
xs.push(20);
xs.push(30);
console.log(String(unwrap_or(__list_get(xs, 0), 0)));
console.log(String(unwrap_or(__list_get(xs, 2), 0)));
console.log(String(unwrap_or(__list_get(xs, 5), 0)));
console.log(is_none(__list_get(xs, 9)));
console.log(String(unwrap_or(first(xs), 0)));
console.log(String(unwrap_or(last(xs), 0)));
console.log(String(unwrap_or(__list_pop(xs), 0)));
console.log(String(xs.length));
console.log(String(unwrap_or(last(xs), 0)));
let single = [  ];
single.push(7);
console.log(String(unwrap_or(__list_pop(single), 0)));
console.log(is_none(__list_pop(single)));
