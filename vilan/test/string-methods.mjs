function __substring(text, start, end) {
	if (0 <= start && start <= end && end <= text.length) return text.substring(__code_point_floor(text, start), __code_point_floor(text, end));
	throw "substring out of range: the length is " + text.length + " but the range is " + start + ".." + end + " — substring requires 0 <= start <= end <= len and never clamps or swaps; to drop a known affix use strip_prefix/strip_suffix, and for the rest of the string pass s.len() as the end";
}
function __code_point_floor(text, at) {
	return at > 0 && (text.charCodeAt(at) & 0xFC00) === 0xDC00 && (text.charCodeAt(at - 1) & 0xFC00) === 0xD800 ? at - 1 : at;
}
function is_empty(self) {
	return self.length === 0;
}
const s = "Hello, World";
console.log(String(s.length));
console.log(s.includes("World"));
console.log(s.startsWith("Hello"));
console.log(s.endsWith("!"));
console.log(s.toUpperCase());
console.log(s.replaceAll("o", "0"));
console.log(__substring(s, 0, 5));
console.log("ab".repeat(3));
console.log(is_empty("  hi  ".trim()));
console.log(is_empty(""));
for (const part of "a,b,c".split(",")) {
	console.log(part);
}
//# sourceMappingURL=string-methods.mjs.map
