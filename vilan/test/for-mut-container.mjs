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
function next_mut(self) {
	let $b = null;
	if (as_usize(self[1]) < self[0].length) {
		const index = self[1];
		self[1] = self[1] + 1;
		$b = [ 0, __at_view(self[0], as_usize(index), "for-mut-container.vl:18:14") ];
	} else {
		$b = [ 1 ];
	}
	return $b;
}
let counter = [ [ 1, 2, 3 ], 0 ];
const $c = counter;
while (true) {
	const $d = next_mut($c);
	if ($d[0] !== 0) {
		break;
	}
	const element = $d[1];
	element[0][element[1]] = element[0][element[1]] * 10;
}
console.log(String(__at(counter[0], 0, "for-mut-container.vl:32:8")));
console.log(String(__at(counter[0], 1, "for-mut-container.vl:33:8")));
console.log(String(__at(counter[0], 2, "for-mut-container.vl:34:8")));
