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
	let $d = null;
	if (self < b) {
		$d = -1;
	} else {
		let $e = null;
		if (self > b) {
			$e = 1;
		} else {
			$e = 0;
		}
		$d = $e;
	}
	return $d;
}
function hold_in_list(items) {
	return [ __clone(items) ];
}
function hold_in_tuple(items) {
	return [ __clone(items), 1 ];
}
function hold_in_struct(items) {
	return [ __clone(items) ];
}
function hold_in_variant(items) {
	return [ 0, __clone(items) ];
}
function donate() {
	const first = [ 1, 2 ];
	const second = [ 3 ];
	return [ first, second ];
}
function keep_scalars(a, b) {
	return [ a, b ];
}
function first_of(primary, fallback) {
	let $c = null;
	if (primary.length > 0) {
		$c = __clone(primary);
	} else {
		$c = __clone(fallback);
	}
	return $c;
}
function own_through(items) {
	return items;
}
function viewed_of(holder) {
	return __clone(holder[0]);
}
function viewed_projection(holder) {
	return holder;
}
function items_view(holder) {
	return holder[0];
}
function reference_of(holder) {
	return __clone(holder[0]);
}
function called_of(holder) {
	return __clone(items_view(holder));
}
function scalar_of(cell2) {
	return cell2[0];
}
function scalar_projection(cell2) {
	return [ cell2, 0 ];
}
function scalar_forward(value) {
	return value[0][value[1]];
}
function elements_are_independent() {
	let rows = [  ];
	rows.push([ 1, 2 ]);
	let kept = filter(rows, (row) => {
		return true;
	});
	__at(kept, 0, "element-clones.vl:130:2").push(9);
	console.log(String(__at(rows, 0, "element-clones.vl:131:8").length));
	let flipped = reverse(rows);
	__at(flipped, 0, "element-clones.vl:133:2").push(9);
	console.log(String(__at(rows, 0, "element-clones.vl:134:8").length));
	let mapped = map(rows, (row) => {
		return __clone(row);
	});
	__at(mapped, 0, "element-clones.vl:136:2").push(9);
	console.log(String(__at(rows, 0, "element-clones.vl:137:8").length));
	let cells = [  ];
	cells.push([ 5 ]);
	let sorted = __list_sort_by(__clone(cells), (a, b) => {
		return compare(a[0], b[0]);
	});
	__at(sorted, 0, "element-clones.vl:141:2")[0] = 99;
	console.log(String(__at(cells, 0, "element-clones.vl:142:8")[0]));
}
function filter(self, predicate) {
	let result = [  ];
	for (const item of self) {
		if (predicate(item)) {
			result.push(__clone(item));
		}
	}
	return result;
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
function map(self, fn) {
	let result = [  ];
	for (const item of self) {
		result.push(fn(item));
	}
	return result;
}
let source = [ 1, 2 ];
let listed = hold_in_list(source);
__at(listed, 0, "element-clones.vl:148:2").push(9);
console.log(String(source.length));
let tupled = hold_in_tuple(source);
tupled[0].push(9);
console.log(String(source.length));
let held = hold_in_struct(source);
held[0].push(9);
console.log(String(source.length));
let wrapped = hold_in_variant(source);
source.push(9);
const $a = wrapped;
let $b = null;
if ($a[0] === 0) {
	const inner = $a[1];
	$b = console.log(String(inner.length));
} else {
	$b = console.log(String(0));
}
$b;
console.log(String(donate().length));
console.log(String(keep_scalars(4, 6)[0]));
let chosen = first_of(source, [ 7 ]);
chosen.push(9);
console.log(String(source.length));
let owned = own_through([ 1, 2 ]);
owned.push(9);
console.log(String(owned.length));
let viewer = [ [ 1, 2 ] ];
let lifted = viewed_of(viewer);
lifted.push(9);
console.log(String(viewer[0].length));
viewed_projection(viewer)[0].push(9);
console.log(String(viewer[0].length));
const referenced = reference_of(viewer);
console.log(String(referenced.length));
const called = called_of(viewer);
console.log(String(called.length));
let cell = [ 5 ];
console.log(String(scalar_of(cell)));
const slot = scalar_projection(cell);
slot[0][slot[1]] = 7;
console.log(String(cell[0]));
let counter = [ 3 ];
console.log(String(scalar_forward([ counter, 0 ])));
elements_are_independent();
