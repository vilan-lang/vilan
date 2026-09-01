function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function make() {
	return [ 4, 5 ];
}
function $a(items) {
	return __clone(items);
}
function $b(items) {
	return 1;
}
function $e(xs) {
	return 1;
}
function $d(items) {
	return $e([ ...__clone(items) ]);
}
const pair = [ 1, 2 ];
const lead = [ ...__clone(pair), 3 ];
const trail = [ 0, ...__clone(pair) ];
const mid = [ 0, ...__clone(pair), 9 ];
const twice = [ ...__clone(pair), ...__clone(pair) ];
const lone = [ ...__clone(pair) ];
console.log(lead[2]);
console.log(trail[2]);
console.log(mid[3]);
console.log(twice[3]);
console.log(lone[1]);
const outer = [ ...__clone(pair), 3 ];
const kept = [ ...outer, 4 ];
console.log(kept[1]);
console.log(kept[3]);
const none = $a([  ]);
console.log([ ...none, 7 ][0]);
console.log([ ...make(), 6 ][2]);
console.log($b([ ...__clone(pair) ]));
console.log($b([ ...pair, 7 ]));
console.log($d([ 1, 2, 3 ]));
