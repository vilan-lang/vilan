function slot(self) {
	return [ self, 0 ];
}
let w = [ 1 ];
const $a = slot(w);
$a[0][$a[1]] = 10;
console.log(String(w[0]));
const v = slot(w);
console.log(String(v[0][v[1]]));
v[0][v[1]] = 25;
console.log(String(w[0]));
