function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __json_kind(value) {
	if (value === null) return "null";
	if (Array.isArray(value)) return "array";
	return typeof value;
}
function __json_tag(value) {
	if (typeof value === "string") return value;
	if (value === null || typeof value !== "object" || Array.isArray(value)) return "";
	return Object.keys(value)[0] ?? "";
}
function __list_pop(list) {
	return list.length === 0 ? [ 1 ] : [ 0, list.pop() ];
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function __sleep(ms, signal) {
	const sig = signal && signal[0] === 0 ? signal[1] : undefined;
	return new Promise((resolve, reject) => {
		if (sig && sig.aborted) {
			reject(sig.reason);
			return;
		}
		const timer = setTimeout(() => resolve(), ms);
		if (sig) sig.addEventListener("abort", () => {
			clearTimeout(timer);
			reject(sig.reason);
		}, { once: true });
	});
}
function __try_parse_json(text) {
	try {
		return [ 0, JSON.parse(text) ];
	} catch (error) {
		return [ 1 ];
	}
}
function new2() {
	return [ new Uint8Array(64), 0 ];
}
function ensure(self, extra) {
	const needed = self[1] + extra;
	let capacity = self[0].length;
	if (needed > capacity) {
		while (needed > capacity) {
			capacity = capacity * 2;
		}
		const grown = new Uint8Array(capacity);
		grown.set(self[0], 0);
		self[0] = grown;
	}
}
function write_byte(self, value2) {
	ensure(self, 1);
	set(self[0], self[1], value2);
	self[1] = self[1] + 1;
}
function write_i32(self, value2) {
	write_byte(self, value2);
	write_byte(self, value2 >> 8);
	write_byte(self, value2 >> 16);
	write_byte(self, value2 >> 24);
}
function write_byte_u32(self, value2) {
	ensure(self, 1);
	set_u32(self[0], self[1], value2);
	self[1] = self[1] + 1;
}
function write_u32(self, value2) {
	write_byte_u32(self, (value2 & 0xFF) >>> 0);
	write_byte_u32(self, (value2 >>> 8 & 0xFF) >>> 0);
	write_byte_u32(self, (value2 >>> 16 & 0xFF) >>> 0);
	write_byte_u32(self, value2 >>> 24);
}
function write_f64(self, value2) {
	const scratch = new Uint8Array(8);
	new DataView(scratch.buffer).setFloat64(0, value2, true);
	ensure(self, 8);
	self[0].set(scratch, self[1]);
	self[1] = self[1] + 8;
}
function write_str(self, value2) {
	const encoded = encode_utf8(value2);
	write_i32(self, as_i322(encoded.length));
	ensure(self, encoded.length);
	self[0].set(encoded, self[1]);
	self[1] = self[1] + encoded.length;
}
function finish(self) {
	return self[0].slice(0, self[1]);
}
function begin_struct(self, fields) {

}
function field(self, name) {

}
function end_struct(self) {

}
function begin_list(self, length) {
	write_i32(self, length);
}
function end_list(self) {

}
function begin_variant(self, name, arity) {
	write_str(self, name);
}
function end_variant(self) {

}
function null_value(self) {
	write_byte(self, 0);
}
function some_value(self) {
	write_byte(self, 1);
}
function str_value(self, value2) {
	write_str(self, value2);
}
function i32_value(self, value2) {
	write_i32(self, value2);
}
function u32_value(self, value2) {
	write_u32(self, value2);
}
function i53_value(self, value2) {
	write_f64(self, Number(value2));
}
function f64_value(self, value2) {
	write_f64(self, value2);
}
function bool_value(self, value2) {
	const $Z = self;
	let $Y = null;
	if (value2) {
		$Y = 1;
	} else {
		$Y = 0;
	}
	write_byte($Z, $Y);
}
function new3(bytes) {
	return [ __clone(bytes), 0, [ 1 ] ];
}
function ok(self) {
	const $ae = self[2];
	let $af = null;
	if ($ae[0] === 0) {
		const _reason = $ae[1];
		$af = false;
	} else {
		$af = true;
	}
	return $af;
}
function report(self, reason) {
	const $ac = self[2];
	let $ad = null;
	if ($ac[0] === 0) {
		const _first = $ac[1];
		$ad = undefined;
	} else {
		self[2] = [ 0, reason ];
		$ad = undefined;
	}
	return $ad;
}
function expect(self, count) {
	if (!(ok(self))) {
		return false;
	}
	if (self[1] + count > self[0].length) {
		report(self, "unexpected end of frame");
		return false;
	}
	return true;
}
function read_byte(self) {
	if (!(expect(self, 1))) {
		return 0;
	}
	const value2 = self[0].at(self[1]);
	self[1] = self[1] + 1;
	return value2;
}
function read_i32(self) {
	if (!(expect(self, 4))) {
		return 0;
	}
	const at = self[1];
	const value2 = self[0].at(at) | self[0].at(at + 1) << 8 | self[0].at(at + 2) << 16 | self[0].at(at + 3) << 24;
	self[1] = at + 4;
	return value2;
}
function read_u32(self) {
	if (!(expect(self, 4))) {
		return 0;
	}
	const at = self[1];
	const value2 = (((self[0].at(at) | self[0].at(at + 1) << 8 >>> 0) >>> 0 | self[0].at(at + 2) << 16 >>> 0) >>> 0 | self[0].at(at + 3) << 24 >>> 0) >>> 0;
	self[1] = at + 4;
	return value2;
}
function read_f64(self) {
	if (!(expect(self, 8))) {
		return 0.0;
	}
	const at = self[1];
	const scratch = self[0].slice(at, at + 8);
	self[1] = at + 8;
	return new DataView(scratch.buffer).getFloat64(0, true);
}
function read_length(self) {
	const length = read_i32(self);
	if (length < 0 || self[1] + as_usize(length) > self[0].length) {
		report(self, "length prefix exceeds frame");
		return 0;
	}
	return as_usize(length);
}
function read_str(self) {
	const length = read_length(self);
	if (!(expect(self, length))) {
		return "";
	}
	const at = self[1];
	const piece = self[0].slice(at, at + length);
	self[1] = at + length;
	return decode_utf8(piece);
}
function begin_struct2(self) {

}
function field2(self, name) {

}
function end_struct2(self) {

}
function begin_list2(self) {
	return as_i322(read_length(self));
}
function end_list2(self) {

}
function variant_tag(self) {
	return read_str(self);
}
function begin_variant2(self, name, arity) {

}
function end_variant2(self) {

}
function is_null(self) {
	if (!(expect(self, 1))) {
		return false;
	}
	const marker = self[0].at(self[1]);
	if (marker === 0) {
		return true;
	}
	if (marker !== 1) {
		report(self, "expected an Option marker (0 or 1), found " + marker);
		return false;
	}
	self[1] = self[1] + 1;
	return false;
}
function null_value2(self) {
	read_byte(self);
}
function str_value2(self) {
	return read_str(self);
}
function i32_value2(self) {
	return read_i32(self);
}
function u32_value2(self) {
	return read_u32(self);
}
function i53_value2(self) {
	return as_i53(read_f64(self));
}
function f64_value2(self) {
	return read_f64(self);
}
function bool_value2(self) {
	const byte = read_byte(self);
	if (byte > 1) {
		report(self, "expected a boolean (0 or 1), found " + byte);
		return false;
	}
	return byte === 1;
}
function fail(self, reason) {
	report(self, reason);
}
function failed(self) {
	return self[2];
}
function binary_codec() {
	return [ () => {
		let writer = new2();
		const record = [ (fields) => {
			return begin_struct(writer, fields);
		}, (name) => {
			return field(writer, name);
		}, () => {
			return end_struct(writer);
		}, (length) => {
			return begin_list(writer, length);
		}, () => {
			return end_list(writer);
		}, (name, arity) => {
			return begin_variant(writer, name, arity);
		}, () => {
			return end_variant(writer);
		}, () => {
			return null_value(writer);
		}, () => {
			return some_value(writer);
		}, (value2) => {
			return str_value(writer, value2);
		}, (value2) => {
			return i32_value(writer, value2);
		}, (value2) => {
			return u32_value(writer, value2);
		}, (value2) => {
			return i53_value(writer, value2);
		}, (value2) => {
			return f64_value(writer, value2);
		}, (value2) => {
			return bool_value(writer, value2);
		} ];
		return [ record, () => {
			return [ 1, finish(writer) ];
		} ];
	}, (frame) => {
		const $aa = frame;
		let $ab = null;
		if ($aa[0] === 1) {
			const bytes = $aa[1];
			$ab = new3(bytes);
		} else {
			const text = $aa[1];
			let poisoned = new3(new Uint8Array(0));
			report(poisoned, "binary codec: received a text frame");
			$ab = poisoned;
		}
		let reader = $ab;
		return [ () => {
			return begin_struct2(reader);
		}, (name) => {
			return field2(reader, name);
		}, () => {
			return end_struct2(reader);
		}, () => {
			return begin_list2(reader);
		}, () => {
			return end_list2(reader);
		}, () => {
			return variant_tag(reader);
		}, (name, arity) => {
			return begin_variant2(reader, name, arity);
		}, () => {
			return end_variant2(reader);
		}, () => {
			return is_null(reader);
		}, () => {
			return null_value2(reader);
		}, () => {
			return str_value2(reader);
		}, () => {
			return i32_value2(reader);
		}, () => {
			return u32_value2(reader);
		}, () => {
			return i53_value2(reader);
		}, () => {
			return f64_value2(reader);
		}, () => {
			return bool_value2(reader);
		}, (reason) => {
			return fail(reader, reason);
		}, () => {
			return failed(reader);
		} ];
	} ];
}
function set(self, index, value2) {
	self.fill(value2, index, index + 1);
}
function set_u32(self, index, value2) {
	self.fill(value2, index, index + 1);
}
function encode_utf8(text) {
	return new TextEncoder().encode(text);
}
function decode_utf8(bytes) {
	return new TextDecoder().decode(bytes);
}
function new4() {
	return [ "", false, [  ], [  ] ];
}
function value(self, text) {
	if (self[1]) {
		self[0] = self[0] + ",";
	}
	self[0] = self[0] + text;
	self[1] = true;
}
function open(self, opener) {
	value(self, opener);
	self[2].push(true);
	self[1] = false;
}
function close(self, closer) {
	self[0] = self[0] + closer;
	const $j = __list_pop(self[2]);
	let $k = null;
	if ($j[0] === 0) {
		const saved = $j[1];
		$k = saved;
	} else {
		$k = false;
	}
	self[1] = $k;
}
function result(self) {
	return self[0];
}
function begin_struct3(self, fields) {
	open(self, "{");
}
function field3(self, name) {
	if (self[1]) {
		self[0] = self[0] + ",";
	}
	self[0] = self[0] + JSON.stringify(name) + ":";
	self[1] = false;
}
function end_struct3(self) {
	close(self, "}");
}
function begin_list3(self, length) {
	open(self, "[");
}
function end_list3(self) {
	close(self, "]");
}
function begin_variant3(self, name, arity) {
	self[3].push(arity);
	let $l = null;
	if (arity === 0) {
		value(self, JSON.stringify(name));
	} else {
		open(self, "{");
		self[0] = self[0] + JSON.stringify(name) + ":";
		self[1] = false;
		if (arity > 1) {
			self[0] = self[0] + "[";
		}
		$l = undefined;
	}
	return $l;
}
function end_variant3(self) {
	const $m = __list_pop(self[3]);
	let $n = null;
	if ($m[0] === 0) {
		const opened = $m[1];
		$n = opened;
	} else {
		$n = 0;
	}
	const arity = $n;
	if (arity > 1) {
		self[0] = self[0] + "]";
	}
	if (arity > 0) {
		close(self, "}");
	}
}
function null_value3(self) {
	value(self, "null");
}
function some_value2(self) {

}
function str_value3(self, value2) {
	value(self, JSON.stringify(value2));
}
function i32_value3(self, value2) {
	value(self, "" + value2);
}
function u32_value3(self, value2) {
	value(self, "" + value2);
}
function i53_value3(self, value2) {
	value(self, "" + value2);
}
function f64_value3(self, value2) {
	value(self, "" + value2);
}
function bool_value3(self, value2) {
	value(self, "" + value2);
}
function new5(root) {
	let stack = [  ];
	stack.push(__clone(root));
	return [ stack, [ 1 ], [  ] ];
}
function ok2(self) {
	const $u = self[1];
	let $v = null;
	if ($u[0] === 0) {
		const _reason = $u[1];
		$v = false;
	} else {
		$v = true;
	}
	return $v;
}
function report2(self, reason) {
	const $s = self[1];
	let $t = null;
	if ($s[0] === 0) {
		const _first = $s[1];
		$t = undefined;
	} else {
		self[1] = [ 0, reason ];
		$t = undefined;
	}
	return $t;
}
function top(self) {
	let $w = null;
	if (!(ok2(self)) || is_empty(self[0])) {
		$w = JSON.parse("null");
	} else {
		$w = __clone(__at(self[0], self[0].length - 1, "std/src/json.vl:521:4"));
	}
	return $w;
}
function take(self) {
	if (!(ok2(self))) {
		return JSON.parse("null");
	}
	const $A = __list_pop(self[0]);
	let $B = null;
	if ($A[0] === 0) {
		const value2 = $A[1];
		$B = value2;
	} else {
		report2(self, "unexpected end of document");
		$B = JSON.parse("null");
	}
	return $B;
}
function expect2(self, value2, wanted, name) {
	if (!(ok2(self))) {
		return false;
	}
	if (__json_kind(value2) === wanted) {
		return true;
	}
	report2(self, "expected " + name + ", found " + found_kind(value2));
	return false;
}
function expect_integer(self, value2, signed) {
	if (!(expect2(self, value2, "number", "a number"))) {
		return false;
	}
	const $M = integer_lane_failure(value2, signed);
	let $N = null;
	if ($M[0] === 0) {
		const reason = $M[1];
		report2(self, reason);
		$N = false;
	} else {
		$N = true;
	}
	return $N;
}
function integer_lane_failure(value2, signed) {
	const number = Number(value2);
	if (number !== Math.floor(number)) {
		return [ 0, "expected a whole number, found " + number ];
	}
	if (!(signed) && number < 0.0) {
		return [ 0, "expected a non-negative number, found " + number ];
	}
	return [ 1 ];
}
function found_kind(value2) {
	const $x = __json_kind(value2);
	let $y = null;
	if ($x === "null") {
		$y = "null";
	} else if ($x === "boolean") {
		$y = "a boolean";
	} else if ($x === "number") {
		$y = "a number";
	} else if ($x === "string") {
		$y = "a string";
	} else if ($x === "array") {
		$y = "an array";
	} else {
		$y = "an object";
	}
	return $y;
}
function begin_struct4(self) {

}
function field4(self, name) {
	const subject = top(self);
	let $z = null;
	if (expect2(self, subject, "object", "an object")) {
		if (Object.hasOwn(subject, name)) {
			self[0].push(subject[name]);
		} else {
			report2(self, "missing field \'" + name + "\'");
		}
		$z = undefined;
	}
	return $z;
}
function end_struct4(self) {
	take(self);
}
function begin_list4(self) {
	const subject = take(self);
	let $E = null;
	if (expect2(self, subject, "array", "an array")) {
		const elements = subject;
		self[2].push(self[0].length);
		let index = elements.length;
		while (index > 0) {
			index = index - 1;
			self[0].push(__clone(__at(elements, index, "std/src/json.vl:669:21")));
		}
		$E = as_i322(elements.length);
	} else {
		$E = 0;
	}
	return $E;
}
function end_list4(self) {
	const $F = __list_pop(self[2]);
	let $G = null;
	if ($F[0] === 0) {
		const mark = $F[1];
		if (ok2(self) && self[0].length > mark) {
			const unread = self[0].length - mark;
			report2(self, "a list had " + unread + " element(s) left unread");
		}
		$G = undefined;
	} else {
		$G = undefined;
	}
	return $G;
}
function variant_tag2(self) {
	let $H = null;
	if (ok2(self)) {
		$H = __json_tag(top(self));
	} else {
		$H = "";
	}
	return $H;
}
function begin_variant4(self, name, arity) {
	const subject = take(self);
	let $K = null;
	if (arity > 0 && expect2(self, subject, "object", "an object")) {
		let $J = null;
		if (Object.hasOwn(subject, name)) {
			const payload = subject[name];
			let $I = null;
			if (arity === 1) {
				self[0].push(payload);
			} else {
				const elements = payload;
				let index = elements.length;
				while (index > 0) {
					index = index - 1;
					self[0].push(__clone(__at(elements, index, "std/src/json.vl:719:23")));
				}
				$I = undefined;
			}
			$J = $I;
		} else {
			report2(self, "missing payload for variant \'" + name + "\'");
		}
		$K = $J;
	}
	return $K;
}
function end_variant4(self) {

}
function is_null2(self) {
	return ok2(self) && top(self) === null;
}
function null_value4(self) {
	take(self);
}
function str_value4(self) {
	const value2 = take(self);
	let $L = null;
	if (expect2(self, value2, "string", "a string")) {
		$L = String(value2);
	} else {
		$L = "";
	}
	return $L;
}
function i32_value4(self) {
	const value2 = take(self);
	let $O = null;
	if (expect_integer(self, value2, true)) {
		$O = Number(value2);
	} else {
		$O = 0;
	}
	return $O;
}
function u32_value4(self) {
	const value2 = take(self);
	let $P = null;
	if (expect_integer(self, value2, false)) {
		$P = Number(value2);
	} else {
		$P = 0;
	}
	return $P;
}
function i53_value4(self) {
	const value2 = take(self);
	let $Q = null;
	if (expect_integer(self, value2, true)) {
		$Q = Number(value2);
	} else {
		$Q = 0;
	}
	return $Q;
}
function f64_value4(self) {
	const value2 = take(self);
	let $R = null;
	if (expect2(self, value2, "number", "a number")) {
		$R = Number(value2);
	} else {
		$R = 0.0;
	}
	return $R;
}
function bool_value4(self) {
	const value2 = take(self);
	let $S = null;
	if (expect2(self, value2, "boolean", "a boolean")) {
		$S = Boolean(value2);
	} else {
		$S = false;
	}
	return $S;
}
function fail2(self, reason) {
	report2(self, reason);
}
function failed2(self) {
	return self[1];
}
function opened_reader(text) {
	const $q = __try_parse_json(text);
	let $r = null;
	if ($q[0] === 0) {
		const root = $q[1];
		$r = new5(root);
	} else {
		let reader = new5(JSON.parse("null"));
		report2(reader, "malformed JSON");
		$r = reader;
	}
	return $r;
}
function json_codec() {
	return [ () => {
		let writer = new4();
		const record = [ (fields) => {
			return begin_struct3(writer, fields);
		}, (name) => {
			return field3(writer, name);
		}, () => {
			return end_struct3(writer);
		}, (length) => {
			return begin_list3(writer, length);
		}, () => {
			return end_list3(writer);
		}, (name, arity) => {
			return begin_variant3(writer, name, arity);
		}, () => {
			return end_variant3(writer);
		}, () => {
			return null_value3(writer);
		}, () => {
			return some_value2(writer);
		}, (value2) => {
			return str_value3(writer, value2);
		}, (value2) => {
			return i32_value3(writer, value2);
		}, (value2) => {
			return u32_value3(writer, value2);
		}, (value2) => {
			return i53_value3(writer, value2);
		}, (value2) => {
			return f64_value3(writer, value2);
		}, (value2) => {
			return bool_value3(writer, value2);
		} ];
		return [ record, () => {
			return [ 0, result(writer) ];
		} ];
	}, (frame) => {
		const $o = frame;
		let $p = null;
		if ($o[0] === 0) {
			const text = $o[1];
			$p = opened_reader(text);
		} else {
			const bytes = $o[1];
			$p = opened_reader(decode_utf8(bytes));
		}
		let reader = $p;
		return [ () => {
			return begin_struct4(reader);
		}, (name) => {
			return field4(reader, name);
		}, () => {
			return end_struct4(reader);
		}, () => {
			return begin_list4(reader);
		}, () => {
			return end_list4(reader);
		}, () => {
			return variant_tag2(reader);
		}, (name, arity) => {
			return begin_variant4(reader, name, arity);
		}, () => {
			return end_variant4(reader);
		}, () => {
			return is_null2(reader);
		}, () => {
			return null_value4(reader);
		}, () => {
			return str_value4(reader);
		}, () => {
			return i32_value4(reader);
		}, () => {
			return u32_value4(reader);
		}, () => {
			return i53_value4(reader);
		}, () => {
			return f64_value4(reader);
		}, () => {
			return bool_value4(reader);
		}, (reason) => {
			return fail2(reader, reason);
		}, () => {
			return failed2(reader);
		} ];
	} ];
}
function partial_compare(self, b) {
	let $a = null;
	if (self < b) {
		$a = [ 0, -1 ];
	} else {
		let $b = null;
		if (self > b) {
			$b = [ 0, 1 ];
		} else {
			$b = [ 0, 0 ];
		}
		$a = $b;
	}
	return $a;
}
function fold_unsigned(value2, modulus) {
	const truncated = Math.trunc(value2);
	const wrapped = truncated % modulus;
	let $C = null;
	if (wrapped < 0) {
		$C = wrapped + modulus;
	} else {
		$C = wrapped;
	}
	return $C;
}
function saturate_unsigned(value2) {
	const truncated = Math.trunc(value2);
	let $ag = null;
	if (truncated > 0) {
		$ag = truncated;
	} else {
		$ag = 0;
	}
	return $ag;
}
function fold_signed(value2, modulus, half) {
	const wrapped = fold_unsigned(value2, modulus);
	let $D = null;
	if (wrapped >= half) {
		$D = wrapped - modulus;
	} else {
		$D = wrapped;
	}
	return $D;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function as_i322(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function as_i53(self) {
	const widened = self;
	return Number(Math.trunc(widened));
}
function now() {
	return [ as_i53(Date.now()) ];
}
function since(self, earlier) {
	return [ self[0] - earlier[0] ];
}
function to_iso(self) {
	return new Date(self[0]).toISOString();
}
function add(self, b) {
	return [ self[0] + b[0] ];
}
function sub(self, b) {
	return [ self[0] - b[0] ];
}
function partial_compare2(self, b) {
	return partial_compare(self[0], b[0]);
}
function millis(count) {
	return [ count ];
}
function seconds(count) {
	return [ count * 1000 ];
}
function minutes(count) {
	return [ count * 60000 ];
}
function hours(count) {
	return [ count * 3600000 ];
}
function days(count) {
	return [ count * 86400000 ];
}
function as_minutes(self) {
	return Math.trunc(self[0] / 60000);
}
function as_hours(self) {
	return Math.trunc(self[0] / 3600000);
}
function as_days(self) {
	return Math.trunc(self[0] / 86400000);
}
function describe(self) {
	let $e = null;
	if (self[0] < 0) {
		$e = "-";
	} else {
		$e = "";
	}
	const sign = $e;
	let $f = null;
	if (self[0] < 0) {
		$f = 0 - self[0];
	} else {
		$f = self[0];
	}
	const magnitude = $f;
	const parts = split_units(magnitude);
	return sign + parts;
}
function split_units(millis2) {
	const days2 = Math.trunc(millis2 / 86400000);
	const hours2 = Math.trunc(millis2 / 3600000) % 24;
	const minutes2 = Math.trunc(millis2 / 60000) % 60;
	const seconds2 = Math.trunc(millis2 / 1000) % 60;
	let $h = null;
	if (days2 > 0) {
		$h = join_units("" + days2 + "d", hours2, "h");
	} else if (hours2 > 0) {
		$h = join_units("" + hours2 + "h", minutes2, "m");
	} else if (minutes2 > 0) {
		$h = join_units("" + minutes2 + "m", seconds2, "s");
	} else if (seconds2 > 0) {
		$h = "" + seconds2 + "s";
	} else {
		$h = "" + millis2 + "ms";
	}
	return $h;
}
function join_units(head, count, unit) {
	let $g = null;
	if (count > 0) {
		$g = "" + head + " " + count + unit;
	} else {
		$g = head;
	}
	return $g;
}
function add2(self, b) {
	return [ self[0] + b[0] ];
}
function sub2(self, b) {
	return [ self[0] - b[0] ];
}
function partial_compare3(self, b) {
	return partial_compare(self[0], b[0]);
}
async function sleep(ms, $ap) {
	await (__sleep(clamp_delay(ms), ambient_signal($ap)));
}
function clamp_delay(ms) {
	let $aq = null;
	if (ms < 0) {
		$aq = 0;
	} else {
		$aq = ms;
	}
	return $aq;
}
async function sleep_for(duration, $ao) {
	await (sleep(as_i32(duration[0]), $ao));
}
function eq(self, other) {
	return self[0] === other[0];
}
function ambient_signal($ar) {
	const $as = $ar;
	let $at = null;
	if ($as[0] === 0) {
		const n = $as[1];
		$at = [ 0, n.signal_of() ];
	} else {
		$at = [ 1 ];
	}
	return $at;
}
function begin_struct5(self, fields) {
	self[0](fields);
}
function field5(self, name) {
	self[1](name);
}
function end_struct5(self) {
	self[2]();
}
function str_value5(self, value2) {
	self[9](value2);
}
function i53_value5(self, value2) {
	self[12](value2);
}
function begin_struct6(self) {
	self[0]();
}
function field6(self, name) {
	self[1](name);
}
function end_struct6(self) {
	self[2]();
}
function str_value6(self) {
	return self[10]();
}
function i53_value6(self) {
	return self[13]();
}
function eq2(self, other) {
	return self[0] === other[0] && self[1] === other[1];
}
function gt(self, b) {
	const $c = partial_compare2(self, b);
	return $c[0] === 0 && $c[1] > 0;
}
function lt(self, b) {
	const $d = partial_compare2(self, b);
	return $d[0] === 0 && $d[1] < 0;
}
function gt2(self, b) {
	const $i = partial_compare3(self, b);
	return $i[0] === 0 && $i[1] > 0;
}
function is_empty(self) {
	return self.length === 0;
}
function describe2(self, serializer) {
	i53_value5(serializer, self);
}
function describe3(self, serializer) {
	str_value5(serializer, self);
}
function describe4(self, serializer) {
	begin_struct5(serializer, 2);
	field5(serializer, "at");
	describe2(self[0], serializer);
	field5(serializer, "label");
	describe3(self[1], serializer);
	end_struct5(serializer);
}
function encode(codec, value2) {
	const $T = codec[0]();
	const serializer = $T[0];
	const finish2 = $T[1];
	let sink = serializer;
	describe4(value2, sink);
	return finish2();
}
function rebuild(deserializer) {
	return i53_value6(deserializer);
}
function rebuild2(deserializer) {
	return str_value6(deserializer);
}
function rebuild3(deserializer) {
	begin_struct6(deserializer);
	field6(deserializer, "at");
	const __at = rebuild(deserializer);
	field6(deserializer, "label");
	const __label = rebuild2(deserializer);
	end_struct6(deserializer);
	return [ __at, __label ];
}
function decode(codec, frame) {
	let deserializer = codec[1](frame);
	const value2 = rebuild3(deserializer);
	const $U = deserializer[17]();
	let $V = null;
	if ($U[0] === 1) {
		$V = [ 0, value2 ];
	} else {
		const reason = $U[1];
		$V = [ 1, reason ];
	}
	return $V;
}
function describe5(self, serializer) {
	begin_struct5(serializer, 1);
	field5(serializer, "millis");
	describe2(self[0], serializer);
	end_struct5(serializer);
}
function encode2(codec, value2) {
	const $aj = codec[0]();
	const serializer = $aj[0];
	const finish2 = $aj[1];
	let sink = serializer;
	describe5(value2, sink);
	return finish2();
}
function rebuild4(deserializer) {
	begin_struct6(deserializer);
	field6(deserializer, "millis");
	const __millis = rebuild(deserializer);
	end_struct6(deserializer);
	return [ __millis ];
}
function decode2(codec, frame) {
	let deserializer = codec[1](frame);
	const value2 = rebuild4(deserializer);
	const $ak = deserializer[17]();
	let $al = null;
	if ($ak[0] === 1) {
		$al = [ 0, value2 ];
	} else {
		const reason = $ak[1];
		$al = [ 1, reason ];
	}
	return $al;
}
(async () => {
	const epoch = [ 0 ];
	console.log(to_iso(epoch));
	const later = add(add(epoch, hours(2)), minutes(5));
	console.log(String(as_minutes(since(later, epoch))));
	console.log(String(as_hours(since(sub(later, minutes(5)), epoch))));
	console.log(gt(later, epoch));
	console.log(lt(epoch, later));
	console.log(eq(sub(later, minutes(125)), epoch));
	const span = add2(add2(days(1), hours(4)), seconds(30));
	console.log(String(as_hours(span)));
	console.log(describe(span));
	console.log(describe(minutes(5)));
	console.log(describe(add2(seconds(90), millis(500))));
	console.log(describe(millis(980)));
	console.log(describe(millis(0)));
	console.log(describe(sub2(seconds(0), hours(3))));
	console.log(String(as_minutes(seconds(59))));
	console.log(gt2(hours(3), minutes(179)));
	console.log(as_days(since(now(), epoch)) > 19000);
	const stamp = [ 1720656000000, "k5" ];
	const json_back = decode(json_codec(), encode(json_codec(), stamp));
	const $W = json_back;
	let $X = null;
	if ($W[0] === 0) {
		const value2 = $W[1];
		$X = console.log(eq2(value2, stamp));
	} else {
		const reason = $W[1];
		$X = console.log(reason);
	}
	$X;
	const binary_back = decode(binary_codec(), encode(binary_codec(), stamp));
	const $ah = binary_back;
	let $ai = null;
	if ($ah[0] === 0) {
		const value3 = $ah[1];
		$ai = console.log("" + (value3[0] === stamp[0]) + " " + value3[1]);
	} else {
		const reason2 = $ah[1];
		$ai = console.log(reason2);
	}
	$ai;
	const sent = decode2(json_codec(), encode2(json_codec(), later));
	const $am = sent;
	let $an = null;
	if ($am[0] === 0) {
		const value4 = $am[1];
		$an = console.log(eq(value4, later));
	} else {
		const reason3 = $am[1];
		$an = console.log(reason3);
	}
	$an;
	await (sleep_for(millis(10), [ 1 ]));
	console.log("slept");
})().catch(($au) => {
	console.error(String($au));
	process.exit(1);
});
