function __substring(text, start, end) {
	if (0 <= start && start <= end && end <= text.length) return text.substring(__code_point_floor(text, start), __code_point_floor(text, end));
	throw "substring out of range: the length is " + text.length + " but the range is " + start + ".." + end + " — substring requires 0 <= start <= end <= len and never clamps or swaps; to drop a known affix use strip_prefix/strip_suffix, and for the rest of the string pass s.len() as the end";
}
function __code_point_floor(text, at) {
	return at > 0 && (text.charCodeAt(at) & 0xFC00) === 0xDC00 && (text.charCodeAt(at - 1) & 0xFC00) === 0xD800 ? at - 1 : at;
}
function debug(self) {
	const digits = "0123456789abcdef";
	const length = self.length;
	let out = "\"";
	let start = 0;
	let index = 0;
	while (index < length) {
		const unit = self.charCodeAt(index);
		if (unit < 32 || unit === 34 || unit === 92 || unit === 127) {
			const low = as_usize(unit % 16);
			const high = as_usize(Math.trunc(unit / 16));
			let $a = null;
			if (unit === 92) {
				$a = "\\\\";
			} else if (unit === 34) {
				$a = "\\\"";
			} else if (unit === 10) {
				$a = "\\n";
			} else if (unit === 9) {
				$a = "\\t";
			} else if (unit === 13) {
				$a = "\\r";
			} else if (unit === 0) {
				$a = "\\0";
			} else if (high === 0) {
				$a = "\\u{" + __substring(digits, low, low + 1) + "}";
			} else {
				$a = "\\u{" + __substring(digits, high, high + 1) + __substring(digits, low, low + 1) + "}";
			}
			const escaped = $a;
			out = out + __substring(self, start, index) + escaped;
			start = index + 1;
		}
		index = index + 1;
	}
	return out + __substring(self, start, length) + "\"";
}
function as_usize(self) {
	const widened = Number(self);
	return Number(Math.trunc(widened));
}
function debug2(self) {
	return "Point { x = " + JSON.stringify(self[0]) + ", " + "y = " + JSON.stringify(self[1]) + " }";
}
function debug3(self) {
	return "Tagged { label = " + debug(self[0]) + ", " + "at = " + debug2(self[1]) + ", " + "on = " + JSON.stringify(self[2]) + " }";
}
function debug4(self) {
	return "Empty";
}
console.log(debug2([ 1, 2 ]));
const t = [ "hi", [ 3, 4 ], true ];
console.log(debug3(t));
console.log(debug4([  ]));
//# sourceMappingURL=derive-debug.mjs.map
