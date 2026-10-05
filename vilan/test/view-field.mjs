function __replace(target, value) {
	if (Array.isArray(target) && Array.isArray(value)) target.length = value.length;
	return Object.assign(target, value);
}
function bump(slot) {
	slot[0][slot[1]] = slot[0][slot[1]] + 1;
}
let c = [ 1 ];
const v = [ c, 0 ];
v[0][v[1]] = 10;
console.log(String(c[0]));
let p = [ 5, 7 ];
bump([ p, 1 ]);
console.log(String(p[1]));
const r = [ p, 0 ];
console.log(String(r[0][r[1]]));
r[0][r[1]] = r[0][r[1]] * 3;
console.log(String(p[0]));
let q = [ 1, 2 ];
const w = q;
__replace(w, [ 7, 8 ]);
console.log(String(q[0]));
console.log(String(q[1]));
