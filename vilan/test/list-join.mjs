function to_string(self) {
	return self;
}
function to_string2(self) {
	return "" + self;
}
function to_string3(self) {
	return "<" + self[0] + ">";
}
function $a(self, separator) {
	let result = "";
	let first = true;
	for (const item of self) {
		if (first) {
			first = false;
		} else {
			result = result + separator;
		}
		result = result + to_string(item);
	}
	return result;
}
function $b(self, separator) {
	let result = "";
	let first = true;
	for (const item of self) {
		if (first) {
			first = false;
		} else {
			result = result + separator;
		}
		result = result + to_string2(item);
	}
	return result;
}
function $c(self, separator) {
	let result = "";
	let first = true;
	for (const item of self) {
		if (first) {
			first = false;
		} else {
			result = result + separator;
		}
		result = result + to_string3(item);
	}
	return result;
}
console.log($a([ "alpha", "beta", "gamma" ], ", "));
console.log($b([ 1, 2, 3 ], "-"));
console.log($a([ "solo" ], ", "));
let empty = [  ];
console.log($a(empty, ", ") === "");
let tags = [  ];
tags.push([ "red" ]);
tags.push([ "blue" ]);
console.log($c(tags, " "));
