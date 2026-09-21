function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
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
	write_i32(self, encoded.length);
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
	let $ag = null;
	if (value2) {
		$ag = 1;
	} else {
		$ag = 0;
	}
	write_byte(self, $ag);
}
function new3(bytes) {
	return [ __clone(bytes), 0, [ 1 ] ];
}
function ok(self) {
	const $al = self[2];
	let $am = null;
	if ($al[0] === 0) {
		const _reason = $al[1];
		$am = false;
	} else {
		$am = true;
	}
	return $am;
}
function report(self, reason) {
	const $aj = self[2];
	let $ak = null;
	if ($aj[0] === 0) {
		const _first = $aj[1];
		$ak = undefined;
	} else {
		self[2] = [ 0, reason ];
		$ak = undefined;
	}
	return $ak;
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
	if (length < 0 || self[1] + length > self[0].length) {
		report(self, "length prefix exceeds frame");
		return 0;
	}
	return length;
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
	return read_length(self);
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
	let $an = null;
	if (self[0].at(self[1]) === 0) {
		$an = true;
	} else {
		self[1] = self[1] + 1;
		$an = false;
	}
	return $an;
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
	return read_byte(self) !== 0;
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
		const $ah = frame;
		let $ai = null;
		if ($ah[0] === 1) {
			const bytes = $ah[1];
			$ai = new3(bytes);
		} else {
			const text = $ah[1];
			let poisoned = new3(new Uint8Array(0));
			report(poisoned, "binary codec: received a text frame");
			$ai = poisoned;
		}
		let reader = $ai;
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
	const $m = __list_pop(self[2]);
	let $n = null;
	if ($m[0] === 0) {
		const saved = $m[1];
		$n = saved;
	} else {
		$n = false;
	}
	self[1] = $n;
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
	let $o = null;
	if (arity === 0) {
		value(self, JSON.stringify(name));
	} else {
		open(self, "{");
		self[0] = self[0] + JSON.stringify(name) + ":";
		self[1] = false;
		if (arity > 1) {
			self[0] = self[0] + "[";
		}
		$o = undefined;
	}
	return $o;
}
function end_variant3(self) {
	const $p = __list_pop(self[3]);
	let $q = null;
	if ($p[0] === 0) {
		const opened = $p[1];
		$q = opened;
	} else {
		$q = 0;
	}
	const arity = $q;
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
	const $x = self[1];
	let $y = null;
	if ($x[0] === 0) {
		const _reason = $x[1];
		$y = false;
	} else {
		$y = true;
	}
	return $y;
}
function report2(self, reason) {
	const $v = self[1];
	let $w = null;
	if ($v[0] === 0) {
		const _first = $v[1];
		$w = undefined;
	} else {
		self[1] = [ 0, reason ];
		$w = undefined;
	}
	return $w;
}
function top(self) {
	let $A = null;
	if (!(ok2(self)) || $z(self[0])) {
		$A = JSON.parse("null");
	} else {
		$A = __clone(__at(self[0], self[0].length - 1));
	}
	return $A;
}
function take(self) {
	if (!(ok2(self))) {
		return JSON.parse("null");
	}
	const $E = __list_pop(self[0]);
	let $F = null;
	if ($E[0] === 0) {
		const value2 = $E[1];
		$F = value2;
	} else {
		report2(self, "unexpected end of document");
		$F = JSON.parse("null");
	}
	return $F;
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
	const number = Number(value2);
	if (number !== Math.floor(number)) {
		report2(self, "expected a whole number, found " + number);
		return false;
	}
	if (!(signed) && number < 0.0) {
		report2(self, "expected a non-negative number, found " + number);
		return false;
	}
	return true;
}
function found_kind(value2) {
	const $B = __json_kind(value2);
	let $C = null;
	if ($B === "null") {
		$C = "null";
	} else if ($B === "boolean") {
		$C = "a boolean";
	} else if ($B === "number") {
		$C = "a number";
	} else if ($B === "string") {
		$C = "a string";
	} else if ($B === "array") {
		$C = "an array";
	} else {
		$C = "an object";
	}
	return $C;
}
function begin_struct4(self) {

}
function field4(self, name) {
	const subject = top(self);
	let $D = null;
	if (expect2(self, subject, "object", "an object")) {
		if (Object.hasOwn(subject, name)) {
			self[0].push(subject[name]);
		} else {
			report2(self, "missing field \'" + name + "\'");
		}
		$D = undefined;
	}
	return $D;
}
function end_struct4(self) {
	take(self);
}
function begin_list4(self) {
	const subject = take(self);
	let $G = null;
	if (expect2(self, subject, "array", "an array")) {
		const elements = subject;
		self[2].push(self[0].length);
		let index = elements.length - 1;
		while (index >= 0) {
			self[0].push(__clone(__at(elements, index)));
			index = index - 1;
		}
		$G = elements.length;
	} else {
		$G = 0;
	}
	return $G;
}
function end_list4(self) {
	const $H = __list_pop(self[2]);
	let $I = null;
	if ($H[0] === 0) {
		const mark = $H[1];
		if (ok2(self) && self[0].length > mark) {
			const unread = self[0].length - mark;
			report2(self, "a list had " + unread + " element(s) left unread");
		}
		$I = undefined;
	} else {
		$I = undefined;
	}
	return $I;
}
function variant_tag2(self) {
	let $J = null;
	if (ok2(self)) {
		$J = __json_tag(top(self));
	} else {
		$J = "";
	}
	return $J;
}
function begin_variant4(self, name, arity) {
	const subject = take(self);
	let $M = null;
	if (arity > 0 && expect2(self, subject, "object", "an object")) {
		let $L = null;
		if (Object.hasOwn(subject, name)) {
			const payload = subject[name];
			let $K = null;
			if (arity === 1) {
				self[0].push(payload);
			} else {
				const elements = payload;
				let index = elements.length - 1;
				while (index >= 0) {
					self[0].push(__clone(__at(elements, index)));
					index = index - 1;
				}
				$K = undefined;
			}
			$L = $K;
		} else {
			report2(self, "missing payload for variant \'" + name + "\'");
		}
		$M = $L;
	}
	return $M;
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
	let $N = null;
	if (expect2(self, value2, "string", "a string")) {
		$N = String(value2);
	} else {
		$N = "";
	}
	return $N;
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
	const $t = __try_parse_json(text);
	let $u = null;
	if ($t[0] === 0) {
		const root = $t[1];
		$u = new5(root);
	} else {
		let reader = new5(JSON.parse("null"));
		report2(reader, "malformed JSON");
		$u = reader;
	}
	return $u;
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
		const $r = frame;
		let $s = null;
		if ($r[0] === 0) {
			const text = $r[1];
			$s = opened_reader(text);
		} else {
			const bytes = $r[1];
			$s = opened_reader(decode_utf8(bytes));
		}
		let reader = $s;
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
	let $b = null;
	if (self < b) {
		$b = [ 0, -1 ];
	} else {
		let $c = null;
		if (self > b) {
			$c = [ 0, 1 ];
		} else {
			$c = [ 0, 0 ];
		}
		$b = $c;
	}
	return $b;
}
function fold_unsigned(value2, modulus) {
	const truncated = Math.trunc(value2);
	const wrapped = truncated % modulus;
	let $aA = null;
	if (wrapped < 0) {
		$aA = wrapped + modulus;
	} else {
		$aA = wrapped;
	}
	return $aA;
}
function fold_signed(value2, modulus, half) {
	const wrapped = fold_unsigned(value2, modulus);
	let $aB = null;
	if (wrapped >= half) {
		$aB = wrapped - modulus;
	} else {
		$aB = wrapped;
	}
	return $aB;
}
function as_i32(self) {
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
	let $g = null;
	if (self[0] < 0) {
		$g = "-";
	} else {
		$g = "";
	}
	const sign = $g;
	let $h = null;
	if (self[0] < 0) {
		$h = 0 - self[0];
	} else {
		$h = self[0];
	}
	const magnitude = $h;
	const parts = split_units(magnitude);
	return sign + parts;
}
function split_units(millis2) {
	const days2 = Math.trunc(millis2 / 86400000);
	const hours2 = Math.trunc(millis2 / 3600000) % 24;
	const minutes2 = Math.trunc(millis2 / 60000) % 60;
	const seconds2 = Math.trunc(millis2 / 1000) % 60;
	let $j = null;
	if (days2 > 0) {
		$j = join_units("" + days2 + "d", hours2, "h");
	} else if (hours2 > 0) {
		$j = join_units("" + hours2 + "h", minutes2, "m");
	} else if (minutes2 > 0) {
		$j = join_units("" + minutes2 + "m", seconds2, "s");
	} else if (seconds2 > 0) {
		$j = "" + seconds2 + "s";
	} else {
		$j = "" + millis2 + "ms";
	}
	return $j;
}
function join_units(head, count, unit) {
	let $i = null;
	if (count > 0) {
		$i = "" + head + " " + count + unit;
	} else {
		$i = head;
	}
	return $i;
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
async function sleep(ms, $aC) {
	await (__sleep(ms, ambient_signal($aC)));
}
async function sleep_for(duration, $az) {
	await (sleep(as_i32(duration[0]), $az));
}
function eq(self, other) {
	return self[0] === other[0];
}
function ambient_signal($aD) {
	const $aE = $aD;
	let $aF = null;
	if ($aE[0] === 0) {
		const n = $aE[1];
		$aF = [ 0, n.signal_of() ];
	} else {
		$aF = [ 1 ];
	}
	return $aF;
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
function $a(self, b) {
	const $d = partial_compare2(self, b);
	return $d[0] === 0 && $d[1] > 0;
}
function $e(self, b) {
	const $f = partial_compare2(self, b);
	return $f[0] === 0 && $f[1] < 0;
}
function $k(self, b) {
	const $l = partial_compare3(self, b);
	return $l[0] === 0 && $l[1] > 0;
}
function $z(self) {
	return self.length === 0;
}
function $W(self, serializer) {
	i53_value5(serializer, self);
}
function $X(self, serializer) {
	str_value5(serializer, self);
}
function $V(self, serializer) {
	begin_struct5(serializer, 2);
	field5(serializer, "at");
	$W(self[0], serializer);
	field5(serializer, "label");
	$X(self[1], serializer);
	end_struct5(serializer);
}
function $T(codec, value2) {
	const $U = codec[0]();
	const serializer = $U[0];
	const finish2 = $U[1];
	let sink = serializer;
	$V(value2, sink);
	return finish2();
}
function $aa(deserializer) {
	return i53_value6(deserializer);
}
function $ab(deserializer) {
	return str_value6(deserializer);
}
function $Z(deserializer) {
	begin_struct6(deserializer);
	field6(deserializer, "at");
	const at = $aa(deserializer);
	field6(deserializer, "label");
	const label = $ab(deserializer);
	end_struct6(deserializer);
	return [ at, label ];
}
function $Y(codec, frame) {
	let deserializer = codec[1](frame);
	const value2 = $Z(deserializer);
	const $ac = deserializer[17]();
	let $ad = null;
	if ($ac[0] === 1) {
		$ad = [ 0, value2 ];
	} else {
		const reason = $ac[1];
		$ad = [ 1, reason ];
	}
	return $ad;
}
function $as(self, serializer) {
	begin_struct5(serializer, 1);
	field5(serializer, "millis");
	$W(self[0], serializer);
	end_struct5(serializer);
}
function $aq(codec, value2) {
	const $ar = codec[0]();
	const serializer = $ar[0];
	const finish2 = $ar[1];
	let sink = serializer;
	$as(value2, sink);
	return finish2();
}
function $au(deserializer) {
	begin_struct6(deserializer);
	field6(deserializer, "millis");
	const millis2 = $aa(deserializer);
	end_struct6(deserializer);
	return [ millis2 ];
}
function $at(codec, frame) {
	let deserializer = codec[1](frame);
	const value2 = $au(deserializer);
	const $av = deserializer[17]();
	let $aw = null;
	if ($av[0] === 1) {
		$aw = [ 0, value2 ];
	} else {
		const reason = $av[1];
		$aw = [ 1, reason ];
	}
	return $aw;
}
(async () => {
	const epoch = [ 0 ];
	console.log(to_iso(epoch));
	const later = add(add(epoch, hours(2)), minutes(5));
	console.log(as_minutes(since(later, epoch)));
	console.log(as_hours(since(sub(later, minutes(5)), epoch)));
	console.log($a(later, epoch));
	console.log($e(epoch, later));
	console.log(eq(sub(later, minutes(125)), epoch));
	const span = add2(add2(days(1), hours(4)), seconds(30));
	console.log(as_hours(span));
	console.log(describe(span));
	console.log(describe(minutes(5)));
	console.log(describe(add2(seconds(90), millis(500))));
	console.log(describe(millis(980)));
	console.log(describe(millis(0)));
	console.log(describe(sub2(seconds(0), hours(3))));
	console.log(as_minutes(seconds(59)));
	console.log($k(hours(3), minutes(179)));
	console.log(as_days(since(now(), epoch)) > 19000);
	const stamp = [ 1720656000000, "k5" ];
	const json_back = $Y(json_codec(), $T(json_codec(), stamp));
	const $ae = json_back;
	let $af = null;
	if ($ae[0] === 0) {
		const value2 = $ae[1];
		$af = console.log(eq2(value2, stamp));
	} else {
		const reason = $ae[1];
		$af = console.log(reason);
	}
	$af;
	const binary_back = $Y(binary_codec(), $T(binary_codec(), stamp));
	const $ao = binary_back;
	let $ap = null;
	if ($ao[0] === 0) {
		const value3 = $ao[1];
		$ap = console.log("" + (value3[0] === stamp[0]) + " " + value3[1]);
	} else {
		const reason2 = $ao[1];
		$ap = console.log(reason2);
	}
	$ap;
	const sent = $at(json_codec(), $aq(json_codec(), later));
	const $ax = sent;
	let $ay = null;
	if ($ax[0] === 0) {
		const value4 = $ax[1];
		$ay = console.log(eq(value4, later));
	} else {
		const reason3 = $ax[1];
		$ay = console.log(reason3);
	}
	$ay;
	await (sleep_for(millis(10), [ 1 ]));
	console.log("slept");
})().catch(($aG) => {
	console.error(String($aG));
	process.exit(1);
});
