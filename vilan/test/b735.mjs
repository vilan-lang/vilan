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
function from_plain() {
	return __clone(plain.v);
}
function from_plain_early(flag) {
	if (flag) {
		return __clone(plain.v);
	}
	return [  ];
}
function from_lazy() {
	return __clone(__force(loaded).v);
}
function from_field() {
	return __clone(holder[0].v);
}
function from_element() {
	return __clone(__at(cells, 0, "b735.vl:48:2").v);
}
function get_xs() {
	return __clone(xs);
}
function get_ys() {
	return __clone(ys);
}
const plain = __shared_new([ 1, 2 ]);
const loaded = __lazy("loaded", () => {
	return __shared_new([ 1, 2 ]);
});
const holder = [ __shared_new([ 1, 2 ]), 0 ];
const cells = [ __shared_new([ 1, 2 ]) ];
const xs = [ 1, 2 ];
let ys = [ 1, 2 ];
let a = from_plain();
a.push(3);
let b = from_plain_early(true);
b.push(3);
let c = from_lazy();
c.push(3);
let d = from_field();
d.push(3);
let e = from_element();
e.push(3);
let f = get_xs();
f.push(3);
let g = get_ys();
g.push(3);
console.log("" + plain.v.length + " " + __force(loaded).v.length + " " + holder[0].v.length + " " + __at(cells, 0, "b735.vl:75:81").v.length + " " + xs.length + " " + ys.length);
const held_plain = from_plain();
const held_lazy = from_lazy();
const held_field = from_field();
const held_ys = get_ys();
plain.v.push(9);
__force(loaded).v.push(9);
holder[0].v.push(9);
ys.push(9);
console.log("" + held_plain.length + " " + held_lazy.length + " " + held_field.length + " " + held_ys.length);
//# sourceMappingURL=b735.mjs.map
