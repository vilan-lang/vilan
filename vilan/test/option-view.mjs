function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_view(list, index, location) {
	if (index >= 0 && index < list.length) return [ list, index ];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $g = null;
	if (truncated > 0) {
		$g = truncated;
	} else {
		$g = 0;
	}
	return $g;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function get_mut(self) {
	return [ 0, [ self, 0 ] ];
}
function get(self) {
	return [ 0, [ self, 0 ] ];
}
function inner_mut(self) {
	return [ 0, self[0] ];
}
function item_mut(self, index) {
	let $h = null;
	if (as_usize(index) < self[1].length) {
		$h = [ 0, __at_view(self[1], as_usize(index), "option-view.vl:42:14") ];
	} else {
		$h = [ 1 ];
	}
	return $h;
}
let slot = [ 1 ];
const $a = get_mut(slot);
let $b = null;
if ($a[0] === 0) {
	const v = $a[1];
	v[0][v[1]] = 42;
	$b = undefined;
} else {
	$b = undefined;
}
$b;
console.log(String(slot[0]));
const $c = get(slot);
let $d = null;
if ($c[0] === 0) {
	const v2 = $c[1];
	console.log(String(v2[0][v2[1]]));
	$d = undefined;
} else {
	$d = undefined;
}
$d;
let outer = [ [ 1 ], [ 10, 20, 30 ] ];
const $e = inner_mut(outer);
let $f = null;
if ($e[0] === 0) {
	const v3 = $e[1];
	v3[0] = 77;
	$f = undefined;
} else {
	$f = undefined;
}
$f;
console.log(String(outer[0][0]));
const $i = item_mut(outer, 1);
let $j = null;
if ($i[0] === 0) {
	const v4 = $i[1];
	v4[0][v4[1]] = 99;
	$j = undefined;
} else {
	$j = undefined;
}
$j;
console.log(String(__at(outer[1], 1, "option-view.vl:87:8")));
const $k = item_mut(outer, 9);
let $l = null;
if ($k[0] === 0) {
	const v5 = $k[1];
	v5[0][v5[1]] = 0;
	$l = undefined;
} else {
	console.log(String(0));
	$l = undefined;
}
process.exit($l);
