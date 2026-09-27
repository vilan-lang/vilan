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
async function __hmac_sha512(key, data) {
	const imported = await crypto.subtle.importKey("raw", key, { name: "HMAC", hash: "SHA-512" }, false, [ "sign" ]);
	return new Uint8Array(await crypto.subtle.sign("HMAC", imported, data));
}
function __json_kind(value) {
	if (value === null) return "null";
	if (Array.isArray(value)) return "array";
	return typeof value;
}
function __list_pop(list) {
	return list.length === 0 ? [ 1 ] : [ 0, list.pop() ];
}
async function __pbkdf2_sha512(password, salt, iterations, bits) {
	const imported = await crypto.subtle.importKey("raw", password, "PBKDF2", false, [ "deriveBits" ]);
	return new Uint8Array(await crypto.subtle.deriveBits({ name: "PBKDF2", salt, iterations, hash: "SHA-512" }, imported, bits));
}
function __substring(text, start, end) {
	if (0 <= start && start <= end && end <= text.length) return text.substring(start, end);
	throw "substring out of range: the length is " + text.length + " but the range is " + start + ".." + end + " — substring requires 0 <= start <= end <= len and never clamps or swaps; to drop a known affix use strip_prefix/strip_suffix, and for the rest of the string pass s.len() as the end";
}
function __try_parse_json(text) {
	try {
		return [ 0, JSON.parse(text) ];
	} catch (error) {
		return [ 1 ];
	}
}
function char_at(value2) {
	return __substring(alphabet, as_usize(value2), as_usize(value2 + 1));
}
function encode_url(bytes) {
	const total = bytes.length;
	let out = "";
	const $d = new4(0, as_i322(Math.trunc(total / 3)));
	while (true) {
		const $e = next($d);
		if ($e[0] !== 0) {
			break;
		}
		const group = $e[1];
		const base = group * 3;
		const chunk = bytes.at(as_usize(base)) << 16 | bytes.at(as_usize(base + 1)) << 8 | bytes.at(as_usize(base + 2));
		out = out + char_at(chunk >> 18 & 63) + char_at(chunk >> 12 & 63) + char_at(chunk >> 6 & 63) + char_at(chunk & 63);
	}
	const rest = total % 3;
	if (rest === 1) {
		const chunk2 = bytes.at(total - 1) << 16;
		out = out + char_at(chunk2 >> 18 & 63) + char_at(chunk2 >> 12 & 63);
	}
	if (rest === 2) {
		const chunk3 = bytes.at(total - 2) << 16 | bytes.at(total - 1) << 8;
		out = out + char_at(chunk3 >> 18 & 63) + char_at(chunk3 >> 12 & 63) + char_at(chunk3 >> 6 & 63);
	}
	return out;
}
function digit(code) {
	const c = as_i32(code);
	if (c >= 65 && c <= 90) {
		return c - 65;
	}
	if (c >= 97 && c <= 122) {
		return c - 71;
	}
	if (c >= 48 && c <= 57) {
		return c + 4;
	}
	if (c === 45) {
		return 62;
	}
	if (c === 95) {
		return 63;
	}
	return 0 - 1;
}
function decode_url(text) {
	const length = text.length;
	const rest = length % 4;
	if (rest === 1) {
		return [ 1 ];
	}
	const full = Math.trunc(length / 4);
	const $g = rest;
	let $h = null;
	if ($g === 2) {
		$h = 1;
	} else if ($g === 3) {
		$h = 2;
	} else {
		$h = 0;
	}
	const tail_bytes = $h;
	let out = new Uint8Array(full * 3 + as_usize(tail_bytes));
	let write = 0;
	const $i = new4(0, as_i322(full));
	while (true) {
		const $j = next($i);
		if ($j[0] !== 0) {
			break;
		}
		const group = $j[1];
		const base = group * 4;
		const a = digit(text.charCodeAt(as_usize(base)));
		const b = digit(text.charCodeAt(as_usize(base + 1)));
		const c = digit(text.charCodeAt(as_usize(base + 2)));
		const d = digit(text.charCodeAt(as_usize(base + 3)));
		if (a < 0 || b < 0 || c < 0 || d < 0) {
			return [ 1 ];
		}
		const chunk = a << 18 | b << 12 | c << 6 | d;
		set(out, write, chunk >> 16 & 255);
		set(out, write + 1, chunk >> 8 & 255);
		set(out, write + 2, chunk & 255);
		write = write + 3;
	}
	if (rest === 2) {
		const a2 = digit(text.charCodeAt(length - 2));
		const b2 = digit(text.charCodeAt(length - 1));
		if (a2 < 0 || b2 < 0) {
			return [ 1 ];
		}
		set(out, write, (a2 << 6 | b2) >> 4 & 255);
	}
	if (rest === 3) {
		const a3 = digit(text.charCodeAt(length - 3));
		const b3 = digit(text.charCodeAt(length - 2));
		const c2 = digit(text.charCodeAt(length - 1));
		if (a3 < 0 || b3 < 0 || c2 < 0) {
			return [ 1 ];
		}
		const chunk2 = a3 << 12 | b3 << 6 | c2;
		set(out, write, chunk2 >> 10 & 255);
		set(out, write + 1, chunk2 >> 2 & 255);
	}
	return [ 0, out ];
}
function set(self, index, value2) {
	self.fill(value2, index, index + 1);
}
function encode_utf8(text) {
	return new TextEncoder().encode(text);
}
function decode_utf8(bytes) {
	return new TextDecoder().decode(bytes);
}
function equals_constant_time(a, b) {
	if (a.length !== b.length) {
		return false;
	}
	let acc = 0;
	const $x = new4(0, as_i322(a.length));
	while (true) {
		const $y = next($x);
		if ($y[0] !== 0) {
			break;
		}
		const index = $y[1];
		acc = acc | a.at(as_usize(index)) ^ b.at(as_usize(index));
	}
	return acc === 0;
}
function new2() {
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
	const $r = __list_pop(self[2]);
	let $s = null;
	if ($r[0] === 0) {
		const saved = $r[1];
		$s = saved;
	} else {
		$s = false;
	}
	self[1] = $s;
}
function result(self) {
	return self[0];
}
function begin_struct(self, fields) {
	open(self, "{");
}
function field(self, name) {
	if (self[1]) {
		self[0] = self[0] + ",";
	}
	self[0] = self[0] + JSON.stringify(name) + ":";
	self[1] = false;
}
function end_struct(self) {
	close(self, "}");
}
function str_value(self, value2) {
	value(self, JSON.stringify(value2));
}
function bool_value(self, value2) {
	value(self, "" + value2);
}
function new3(root) {
	let stack = [  ];
	stack.push(__clone(root));
	return [ stack, [ 1 ], [  ] ];
}
function ok(self) {
	const $I = self[1];
	let $J = null;
	if ($I[0] === 0) {
		const _reason = $I[1];
		$J = false;
	} else {
		$J = true;
	}
	return $J;
}
function report(self, reason) {
	const $F = self[1];
	let $G = null;
	if ($F[0] === 0) {
		const _first = $F[1];
		$G = undefined;
	} else {
		self[1] = [ 0, reason ];
		$G = undefined;
	}
	return $G;
}
function top(self) {
	let $L = null;
	if (!(ok(self)) || $K(self[0])) {
		$L = JSON.parse("null");
	} else {
		$L = __clone(__at(self[0], self[0].length - 1));
	}
	return $L;
}
function take(self) {
	if (!(ok(self))) {
		return JSON.parse("null");
	}
	const $Q = __list_pop(self[0]);
	let $R = null;
	if ($Q[0] === 0) {
		const value2 = $Q[1];
		$R = value2;
	} else {
		report(self, "unexpected end of document");
		$R = JSON.parse("null");
	}
	return $R;
}
function expect(self, value2, wanted, name) {
	if (!(ok(self))) {
		return false;
	}
	if (__json_kind(value2) === wanted) {
		return true;
	}
	report(self, "expected " + name + ", found " + found_kind(value2));
	return false;
}
function found_kind(value2) {
	const $M = __json_kind(value2);
	let $N = null;
	if ($M === "null") {
		$N = "null";
	} else if ($M === "boolean") {
		$N = "a boolean";
	} else if ($M === "number") {
		$N = "a number";
	} else if ($M === "string") {
		$N = "a string";
	} else if ($M === "array") {
		$N = "an array";
	} else {
		$N = "an object";
	}
	return $N;
}
function begin_struct2(self) {

}
function field2(self, name) {
	const subject = top(self);
	let $O = null;
	if (expect(self, subject, "object", "an object")) {
		if (Object.hasOwn(subject, name)) {
			self[0].push(subject[name]);
		} else {
			report(self, "missing field \'" + name + "\'");
		}
		$O = undefined;
	}
	return $O;
}
function end_struct2(self) {
	take(self);
}
function str_value2(self) {
	const value2 = take(self);
	let $S = null;
	if (expect(self, value2, "string", "a string")) {
		$S = String(value2);
	} else {
		$S = "";
	}
	return $S;
}
function bool_value2(self) {
	const value2 = take(self);
	let $U = null;
	if (expect(self, value2, "boolean", "a boolean")) {
		$U = Boolean(value2);
	} else {
		$U = false;
	}
	return $U;
}
function opened_reader(text) {
	const $D = __try_parse_json(text);
	let $E = null;
	if ($D[0] === 0) {
		const root = $D[1];
		$E = new3(root);
	} else {
		let reader = new3(JSON.parse("null"));
		report(reader, "malformed JSON");
		$E = reader;
	}
	return $E;
}
function fold_unsigned(value2, modulus) {
	const truncated = Math.trunc(value2);
	const wrapped = truncated % modulus;
	let $a = null;
	if (wrapped < 0) {
		$a = wrapped + modulus;
	} else {
		$a = wrapped;
	}
	return $a;
}
function saturate_unsigned(value2) {
	const truncated = Math.trunc(value2);
	let $f = null;
	if (truncated > 0) {
		$f = truncated;
	} else {
		$f = 0;
	}
	return $f;
}
function fold_signed(value2, modulus, half) {
	const wrapped = fold_unsigned(value2, modulus);
	let $b = null;
	if (wrapped >= half) {
		$b = wrapped - modulus;
	} else {
		$b = wrapped;
	}
	return $b;
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
function new4(start, end) {
	return [ start, end ];
}
function next(self) {
	let $c = null;
	if (self[0] < self[1]) {
		const value2 = self[0];
		self[0] = self[0] + 1;
		$c = [ 0, value2 ];
	} else {
		$c = [ 1 ];
	}
	return $c;
}
function $p(self, serializer) {
	str_value(serializer, self);
}
function $q(self, serializer) {
	bool_value(serializer, self);
}
function $o(self, serializer) {
	begin_struct(serializer, 2);
	field(serializer, "user");
	$p(self[0], serializer);
	field(serializer, "admin");
	$q(self[1], serializer);
	end_struct(serializer);
}
function $n(value2) {
	let writer = new2();
	$o(value2, writer);
	return result(writer);
}
async function $m(secret, claims) {
	const payload = encode_url(encode_utf8($n(claims)));
	const signing_input = header_segment + "." + payload;
	const signature = await (__hmac_sha512(secret, encode_utf8(signing_input)));
	return signing_input + "." + encode_url(signature);
}
function $K(self) {
	return self.length === 0;
}
function $P(deserializer) {
	return str_value2(deserializer);
}
function $T(deserializer) {
	return bool_value2(deserializer);
}
function $H(deserializer) {
	begin_struct2(deserializer);
	field2(deserializer, "user");
	const user = $P(deserializer);
	field2(deserializer, "admin");
	const admin = $T(deserializer);
	end_struct2(deserializer);
	return [ user, admin ];
}
function $C(text) {
	let reader = opened_reader(text);
	const value2 = $H(reader);
	const $V = reader[1];
	let $W = null;
	if ($V[0] === 1) {
		$W = [ 0, value2 ];
	} else {
		const reason = $V[1];
		$W = [ 1, reason ];
	}
	return $W;
}
function $z(segment) {
	const $A = decode_url(segment);
	let $B = null;
	if ($A[0] === 0) {
		const payload = $A[1];
		const decoded = $C(decode_utf8(payload));
		const $X = decoded;
		let $Y = null;
		if ($X[0] === 0) {
			const claims = $X[1];
			$Y = [ 0, __clone(claims) ];
		} else {
			const _reason = $X[1];
			$Y = [ 1 ];
		}
		$B = $Y;
	} else {
		$B = [ 1 ];
	}
	return $B;
}
async function $t(secret, token) {
	const parts = token.split(".");
	let $u = null;
	if (parts.length !== 3 || __at(parts, 0) !== header_segment) {
		$u = [ 1 ];
	} else {
		const expected = await (__hmac_sha512(secret, encode_utf8(__at(parts, 0) + "." + __at(parts, 1))));
		const $v = decode_url(__at(parts, 2));
		let $w = null;
		if ($v[0] === 0) {
			const given = $v[1];
			let $Z = null;
			if (equals_constant_time(expected, given)) {
				$Z = $z(__at(parts, 1));
			} else {
				$Z = [ 1 ];
			}
			$w = $Z;
		} else {
			$w = [ 1 ];
		}
		$u = $w;
	}
	return $u;
}
const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const header_segment = "eyJhbGciOiJIUzUxMiIsInR5cCI6IkpXVCJ9";
(async () => {
	const $k = decode_url(encode_url(encode_utf8("payload")));
	let $l = null;
	if ($k[0] === 0) {
		const bytes = $k[1];
		$l = console.log(decode_utf8(bytes));
	} else {
		$l = console.log("decode failed");
	}
	$l;
	const secret = encode_utf8("server-signing-key");
	const token = await ($m(secret, [ "reed", true ]));
	const verified = await ($t(secret, token));
	const $aa = verified;
	let $ab = null;
	if ($aa[0] === 0) {
		const session = $aa[1];
		$ab = console.log("welcome " + session[0] + " (admin=" + session[1] + ")");
	} else {
		$ab = console.log("unauthorized");
	}
	$ab;
	const forged = await ($t(encode_utf8("attacker-key"), token));
	const $ac = forged;
	let $ad = null;
	if ($ac[0] === 0) {
		const _s = $ac[1];
		$ad = console.log("SECURITY BUG");
	} else {
		$ad = console.log("forged token rejected");
	}
	$ad;
	const salt = encode_utf8("per-user-salt");
	const first = await (__pbkdf2_sha512(encode_utf8("hunter2"), salt, 1000, 512));
	const again = await (__pbkdf2_sha512(encode_utf8("hunter2"), salt, 1000, 512));
	console.log(equals_constant_time(first, again));
})().catch(($ae) => {
	console.error(String($ae));
	process.exit(1);
});
