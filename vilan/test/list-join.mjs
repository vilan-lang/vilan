function to_string(self) {
	return self;
}
function to_string2(self) {
	return "" + self;
}
function to_string3(self) {
	return "<" + self[0] + ">";
}
function join(self, separator) {
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
function join2(self, separator) {
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
function join3(self, separator) {
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
console.log(join([ "alpha", "beta", "gamma" ], ", "));
console.log(join2([ 1, 2, 3 ], "-"));
console.log(join([ "solo" ], ", "));
let empty = [  ];
console.log(join(empty, ", ") === "");
let tags = [  ];
tags.push([ "red" ]);
tags.push([ "blue" ]);
console.log(join3(tags, " "));
