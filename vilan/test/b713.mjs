function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_put(list, index, value, location) {
	if (index >= 0 && index < list.length) return list[index] = value;
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
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
function __shared_new(value) {
	return { v: value };
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $a = null;
	if (truncated > 0) {
		$a = truncated;
	} else {
		$a = 0;
	}
	return $a;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function table(self) {
	return self[0];
}
function table2(self) {
	return self[0];
}
function table3(handler) {
	return __shared_new([ [ [ 0 ], [ 0 ] ], __clone(handler) ]);
}
function bump(table4, id) {
	const $b = as_usize(id);
	__at(table4.v[0], $b, "b713.vl:27:2")[0] = __at(table4.v[0], $b, "b713.vl:27:2")[0] + 1;
}
function bump_later(table4, id) {
	return () => {
		const $d = as_usize(id);
		__at(table4.v[0], $d, "b713.vl:34:3")[0] = __at(table4.v[0], $d, "b713.vl:34:3")[0] + 10;
		return;
	};
}
function bump_grid(grid2, tag, row, column) {
	const $f = as_usize(row);
	const $g = as_usize(column);
	__at_put(__at(grid2.v, $f, "b713.vl:40:2"), $g, __at(__at(grid2.v, $f, "b713.vl:40:2"), $g, "b713.vl:40:2") + 100, "b713.vl:40:2");
}
function bump_at(self, id) {
	const $j = as_usize(id);
	__at(table(self).v, $j, "b713.vl:47:3")[0] = __at(table(self).v, $j, "b713.vl:47:3")[0] + 1000;
}
function bump_at2(self, id) {
	const $k = as_usize(id);
	__at(table2(self).v, $k, "b713.vl:47:3")[0] = __at(table2(self).v, $k, "b713.vl:47:3")[0] + 1000;
}
const a = table3(1);
const b = table3("x");
bump(a, 0);
bump(b, 1);
console.log("generic: " + __at(a.v[0], 0, "b713.vl:77:20")[0] + " " + __at(b.v[0], 1, "b713.vl:77:46")[0]);
const later_a = bump_later(a, 1);
const later_b = bump_later(b, 0);
later_a();
later_b();
console.log("closure: " + __at(a.v[0], 1, "b713.vl:83:20")[0] + " " + __at(b.v[0], 0, "b713.vl:83:46")[0]);
const grid = __shared_new([ [ 1, 2 ], [ 3, 4 ] ]);
bump_grid(grid, 1, 0, 1);
bump_grid(grid, "y", 1, 0);
console.log("nested: " + __at(__at(grid.v, 0, "b713.vl:88:19"), 1, "b713.vl:88:19") + " " + __at(__at(grid.v, 1, "b713.vl:88:39"), 0, "b713.vl:88:39"));
const one = [ __shared_new([ [ 0 ] ]) ];
const two = [ __shared_new([ [ 0 ] ]), "two" ];
bump_at(one, 0);
bump_at2(two, 0);
console.log("default method: " + __at(one[0].v, 0, "b713.vl:94:27")[0] + " " + __at(two[0].v, 0, "b713.vl:94:55")[0] + " " + two[1]);
//# sourceMappingURL=b713.mjs.map
