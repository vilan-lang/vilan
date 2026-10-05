function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
let xs = [  ];
xs.push(1);
xs.push(2);
xs.push(3);
const $a = xs;
for (const $b of $a.keys()) {
	const e = [ $a, $b ];
	e[0][e[1]] = e[0][e[1]] * 10;
}
console.log(__at(xs, 0, "for-views.vl:21:8"));
console.log(__at(xs, 2, "for-views.vl:22:8"));
let ps = [  ];
ps.push([ 1 ]);
ps.push([ 2 ]);
const $c = ps;
for (const $d of $c.keys()) {
	const p = $c[$d];
	p[0] = p[0] + 100;
}
console.log(__at(ps, 0, "for-views.vl:31:8")[0]);
console.log(__at(ps, 1, "for-views.vl:32:8")[0]);
let sum = 0;
const $e = xs;
for (const $f of $e.keys()) {
	const e2 = [ $e, $f ];
	sum = sum + e2[0][e2[1]];
}
console.log(sum);
