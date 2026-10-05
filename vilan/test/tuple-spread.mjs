function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function make() {
	return [ 4, 5 ];
}
function pack(items) {
	return __clone(items);
}
function need2(items) {
	return 1;
}
function width(xs) {
	return 1;
}
function forward(items) {
	return width([ ...__clone(items) ]);
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
const none = pack([  ]);
console.log([ ...none, 7 ][0]);
console.log([ ...make(), 6 ][2]);
console.log(need2([ ...__clone(pair) ]));
console.log(need2([ ...pair, 7 ]));
console.log(forward([ 1, 2, 3 ]));
