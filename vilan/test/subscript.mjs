function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_put(list, index, value, location) {
	if (index >= 0 && index < list.length) return list[index] = value;
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_view(list, index, location) {
	if (index >= 0 && index < list.length) return [ list, index ];
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
function bump(slot) {
	slot[0][slot[1]] = slot[0][slot[1]] + 100;
}
let xs = [  ];
xs.push(10);
xs.push(20);
console.log(String(__at(xs, 0, "subscript.vl:23:8") + __at(xs, 1, "subscript.vl:23:16")));
__at_put(xs, 1, 99, "subscript.vl:24:2");
console.log(String(__at(xs, 1, "subscript.vl:25:8")));
const i = 0;
bump(__at_view(xs, i + 0, "subscript.vl:29:12"));
console.log(String(__at(xs, 0, "subscript.vl:30:8")));
let ps = [  ];
ps.push([ 1, 2 ]);
let copy = __clone(__at(ps, 0, "subscript.vl:35:13"));
copy[0] = 7;
console.log(String(__at(ps, 0, "subscript.vl:37:8")[0]));
const view = __at(ps, 0, "subscript.vl:40:18");
view[1] = 50;
console.log(String(__at(ps, 0, "subscript.vl:42:8")[1]));
console.log(String(__at(xs, xs.length - 1, "subscript.vl:45:8")));
