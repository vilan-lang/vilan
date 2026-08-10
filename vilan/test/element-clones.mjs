function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
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
function compare(self, b) {
	let $g = null;
	if (self < b) {
		$g = -1;
	} else {
		let $h = null;
		if (self > b) {
			$h = 1;
		} else {
			$h = 0;
		}
		$g = $h;
	}
	return $g;
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
	let kept = $d(rows, (row) => {
		return true;
	});
	__at(kept, 0).push(9);
	console.log(__at(rows, 0).length);
	let flipped = $e(rows);
	__at(flipped, 0).push(9);
	console.log(__at(rows, 0).length);
	let mapped = $f(rows, (row) => {
		return __clone(row);
	});
	__at(mapped, 0).push(9);
	console.log(__at(rows, 0).length);
	let cells = [  ];
	cells.push([ 5 ]);
	let sorted = __list_sort_by(__clone(cells), (a, b) => {
		return compare(a[0], b[0]);
	});
	__at(sorted, 0)[0] = 99;
	console.log(__at(cells, 0)[0]);
}
function $d(self, predicate) {
	let result = [  ];
	for (const item of self) {
		if (predicate(item)) {
			result.push(__clone(item));
		}
	}
	return result;
}
function $e(self) {
	let result = [  ];
	let index = self.length - 1;
	while (index >= 0) {
		result.push(__clone(__at(self, index)));
		index = index - 1;
	}
	return result;
}
function $f(self, fn) {
	let result = [  ];
	for (const item of self) {
		result.push(fn(item));
	}
	return result;
}
let source = [ 1, 2 ];
let listed = hold_in_list(source);
__at(listed, 0).push(9);
console.log(source.length);
let tupled = hold_in_tuple(source);
tupled[0].push(9);
console.log(source.length);
let held = hold_in_struct(source);
held[0].push(9);
console.log(source.length);
let wrapped = hold_in_variant(source);
source.push(9);
const $a = wrapped;
let $b = null;
if ($a[0] === 0) {
	const inner = $a[1];
	$b = console.log(inner.length);
} else {
	$b = console.log(0);
}
$b;
console.log(donate().length);
console.log(keep_scalars(4, 6)[0]);
let chosen = first_of(source, [ 7 ]);
chosen.push(9);
console.log(source.length);
let owned = own_through([ 1, 2 ]);
owned.push(9);
console.log(owned.length);
let viewer = [ [ 1, 2 ] ];
let lifted = viewed_of(viewer);
lifted.push(9);
console.log(viewer[0].length);
viewed_projection(viewer)[0].push(9);
console.log(viewer[0].length);
const referenced = reference_of(viewer);
console.log(referenced.length);
const called = called_of(viewer);
console.log(called.length);
let cell = [ 5 ];
console.log(scalar_of(cell));
const slot = scalar_projection(cell);
slot[0][slot[1]] = 7;
console.log(cell[0]);
let counter = [ 3 ];
console.log(scalar_forward([ counter, 0 ]));
elements_are_independent();
