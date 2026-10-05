function describe(value) {
	return JSON.stringify(value);
}
function to_json(self) {
	let result = "[";
	let first = true;
	for (const element of self) {
		if (!(first)) {
			result = result + ",";
		}
		result = result + JSON.stringify(element);
		first = false;
	}
	return result + "]";
}
function to_json2(self) {
	const $a = self;
	let $b = null;
	if ($a[0] === 0) {
		const value = $a[1];
		$b = JSON.stringify(value);
	} else {
		$b = "null";
	}
	return $b;
}
console.log(describe(42));
console.log(describe("hi"));
const nums = [ 1, 2, 3 ];
console.log(to_json(nums));
const maybe = [ 0, 7 ];
console.log(to_json2(maybe));
const nothing = [ 1 ];
console.log(to_json2(nothing));
