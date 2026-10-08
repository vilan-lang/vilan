function __shared_new(value) {
	return { v: value };
}
const s = __shared_new([ 0 ]);
const a = s;
const b = s;
a.v[0] = a.v[0] + 1;
a.v[0] = a.v[0] + 1;
console.log(String(b.v[0]));
console.log(String(s.v[0]));
const other = __shared_new([ 100 ]);
other.v[0] = 50;
console.log(String(other.v[0]));
console.log(String(s.v[0]));
