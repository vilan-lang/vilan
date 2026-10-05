function compare(self, b) {
	let $a = null;
	if (self[0] < b[0]) {
		$a = -1;
	} else if (self[0] > b[0]) {
		$a = 1;
	} else {
		$a = 0;
	}
	return $a;
}
function min(self, b) {
	let $b = null;
	if (compare(self, b) <= 0) {
		$b = self;
	} else {
		$b = b;
	}
	return $b;
}
function max(self, b) {
	let $c = null;
	if (compare(self, b) >= 0) {
		$c = self;
	} else {
		$c = b;
	}
	return $c;
}
function clamp(self, min2, max2) {
	return max(min(self, max2), min2);
}
const low = [ 3 ];
const high = [ 7 ];
console.log(min(low, high));
console.log(max(low, high));
const below = [ 1 ];
const above = [ 9 ];
const within = [ 5 ];
console.log(clamp(below, low, high));
console.log(clamp(above, low, high));
console.log(clamp(within, low, high));
