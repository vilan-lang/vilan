function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function __parse_i32(text) {
	const trimmed = text.trim();
	const value = Number(trimmed);
	return /^[+-]?[0-9]+$/.test(trimmed) && value >= -2147483648 && value <= 2147483647 ? [ 0, value ] : [ 1 ];
}
function set(self, index, value) {
	self.fill(value, index, index + 1);
}
function concat(a, b) {
	const joined = new Uint8Array(a.length + b.length);
	joined.set(a, 0);
	joined.set(b, a.length);
	return joined;
}
function decode_utf8(bytes2) {
	return new TextDecoder().decode(bytes2);
}
function debug(self) {
	const escaped = self.replaceAll("\\", "\\\\").replaceAll("\"", "\\\"").replaceAll("\n", "\\n").replaceAll("\t", "\\t").replaceAll("\r", "\\r").replaceAll("\u{0}", "\\0");
	return "\"" + escaped + "\"";
}
function key(self) {
	const $K = self[0];
	let $L = null;
	if ($K[0] === 0) {
		const scalar = $K[1];
		let $M = null;
		if (self[1]) {
			$M = [ 1, scalar ];
		} else if (self[2]) {
			$M = [ 2, scalar ];
		} else {
			$M = [ 0, scalar ];
		}
		$L = $M;
	} else if ($K[0] === 4) {
		let $N = null;
		if (self[3]) {
			$N = [ 5 ];
		} else {
			$N = [ 4 ];
		}
		$L = $N;
	} else {
		const other = $K;
		$L = other;
	}
	return $L;
}
function plain(key2) {
	return [ key2, false, false, false ];
}
function new2() {
	return [ new Uint8Array(0), 0, false, false, false ];
}
function feed(self, bytes2) {
	if (bytes2.length === 0) {
		self[3] = true;
	} else {
		const rest = self[0].slice(self[1], self[0].length);
		self[0] = concat(rest, bytes2);
		self[1] = 0;
	}
}
function finish(self) {
	self[4] = true;
}
function next(self) {
	const $G = step(self, false);
	let $H = null;
	if ($G[0] === 0) {
		const press = $G[1];
		$H = [ 0, press ];
	} else {
		$H = [ 1 ];
	}
	return $H;
}
function step(self, terminal) {
	const resolving = self[3] || self[4];
	self[3] = false;
	while (true) {
		if (self[2] && self[1] < self[0].length) {
			self[2] = false;
			if (self[0].at(self[1]) === 10) {
				self[1] = self[1] + 1;
			}
			continue;
		}
		if (self[1] >= self[0].length) {
			let $c = null;
			if (self[4]) {
				$c = [ 1 ];
			} else {
				$c = [ 2, -(1) ];
			}
			return $c;
		}
		const $D = decode_step(self[0], self[1], resolving, self[4]);
		let $E = null;
		if ($D[0] === 0) {
			const press = $D[1];
			const used = $D[2];
			self[2] = self[0].at(self[1]) === 13;
			self[1] = self[1] + used;
			return [ 0, press ];
			$E = undefined;
		} else if ($D[0] === 1) {
			const used2 = $D[1];
			$E = self[1] = self[1] + used2;
		} else if ($D[0] === 2) {
			let $F = null;
			if (terminal) {
				$F = 50;
			} else {
				$F = -(1);
			}
			return [ 2, $F ];
		} else {
			return [ 2, -(1) ];
		}
		$E;
	}
}
function decode_step(bytes2, at, resolving, ended) {
	const lead = bytes2.at(at);
	let $z = null;
	if (lead === 27) {
		$z = decode_escape(bytes2, at, resolving, ended);
	} else if (lead >= 128) {
		$z = decode_scalar(bytes2, at, ended);
	} else {
		$z = [ 0, control_or_char(lead), 1 ];
	}
	return $z;
}
function control_or_char(byte) {
	let $C = null;
	if (byte === 13 || byte === 10) {
		$C = plain([ 3 ]);
	} else if (byte === 9) {
		$C = plain([ 4 ]);
	} else if (byte === 127 || byte === 8) {
		$C = plain([ 6 ]);
	} else if (byte === 0) {
		$C = [ [ 0, " " ], true, false, false ];
	} else if (byte < 27) {
		$C = [ [ 0, ascii(byte + 96) ], true, false, false ];
	} else if (byte < 32) {
		$C = [ [ 0, ascii(byte + 64) ], true, false, false ];
	} else {
		$C = [ [ 0, ascii(byte) ], false, false, byte >= 65 && byte <= 90 ];
	}
	return $C;
}
function ascii(code) {
	const byte = new Uint8Array(1);
	set(byte, 0, code);
	return decode_utf8(byte);
}
function decode_scalar(bytes2, at, ended) {
	const lead = bytes2.at(at);
	let $A = null;
	if (lead >= 194 && lead <= 223) {
		$A = 2;
	} else if (lead >= 224 && lead <= 239) {
		$A = 3;
	} else if (lead >= 240 && lead <= 244) {
		$A = 4;
	} else {
		$A = 0;
	}
	const width = $A;
	if (width === 0) {
		return [ 1, 1 ];
	}
	const left = bytes2.length - at;
	let index = 1;
	while (index < width) {
		if (index >= left) {
			let $B = null;
			if (ended) {
				$B = [ 1, left ];
			} else {
				$B = [ 3 ];
			}
			return $B;
		}
		const continuation = bytes2.at(at + index);
		if (continuation < 128 || continuation > 191) {
			return [ 1, index ];
		}
		index = index + 1;
	}
	const scalar = decode_utf8(bytes2.slice(at, at + width));
	return [ 0, plain([ 0, scalar ]), width ];
}
function decode_escape(bytes2, at, resolving, ended) {
	const left = bytes2.length - at;
	if (left === 1) {
		let $d = null;
		if (resolving) {
			$d = [ 0, plain([ 9 ]), 1 ];
		} else {
			$d = [ 2 ];
		}
		return $d;
	}
	const second = bytes2.at(at + 1);
	let $t = null;
	if (second === 91) {
		$t = decode_csi(bytes2, at, resolving, ended);
	} else if (second === 79) {
		$t = decode_ss3(bytes2, at, resolving);
	} else if (second === 27) {
		$t = [ 0, plain([ 9 ]), 1 ];
	} else {
		const $x = decode_step(bytes2, at + 1, resolving, ended);
		let $y = null;
		if ($x[0] === 0) {
			const press = $x[1];
			const used = $x[2];
			$y = [ 0, [ press[0], press[1], true, press[3] ], used + 1 ];
		} else if ($x[0] === 1) {
			const used2 = $x[1];
			$y = [ 1, used2 + 1 ];
		} else if ($x[0] === 2) {
			$y = [ 2 ];
		} else {
			$y = [ 3 ];
		}
		$t = $y;
	}
	return $t;
}
function decode_ss3(bytes2, at, resolving) {
	if (bytes2.length - at < 3) {
		let $u = null;
		if (resolving) {
			$u = [ 0, [ [ 0, "O" ], false, true, true ], 2 ];
		} else {
			$u = [ 2 ];
		}
		return $u;
	}
	const final = bytes2.at(at + 2);
	const $v = final_key(final);
	let $w = null;
	if ($v[0] === 0) {
		const key2 = $v[1];
		$w = [ 0, plain(key2), 3 ];
	} else {
		$w = [ 1, 3 ];
	}
	return $w;
}
function final_key(final) {
	let $q = null;
	if (final === 65) {
		$q = [ 0, [ 10 ] ];
	} else if (final === 66) {
		$q = [ 0, [ 11 ] ];
	} else if (final === 67) {
		$q = [ 0, [ 13 ] ];
	} else if (final === 68) {
		$q = [ 0, [ 12 ] ];
	} else if (final === 72) {
		$q = [ 0, [ 14 ] ];
	} else if (final === 70) {
		$q = [ 0, [ 15 ] ];
	} else if (final >= 80 && final <= 83) {
		$q = [ 0, [ 18, final - 79 ] ];
	} else {
		$q = [ 1 ];
	}
	return $q;
}
function tilde_key(number) {
	let $j = null;
	if (number === 1 || number === 7) {
		$j = [ 0, [ 14 ] ];
	} else if (number === 2) {
		$j = [ 0, [ 8 ] ];
	} else if (number === 3) {
		$j = [ 0, [ 7 ] ];
	} else if (number === 4 || number === 8) {
		$j = [ 0, [ 15 ] ];
	} else if (number === 5) {
		$j = [ 0, [ 16 ] ];
	} else if (number === 6) {
		$j = [ 0, [ 17 ] ];
	} else if (number >= 11 && number <= 15) {
		$j = [ 0, [ 18, number - 10 ] ];
	} else if (number >= 17 && number <= 21) {
		$j = [ 0, [ 18, number - 11 ] ];
	} else if (number === 23 || number === 24) {
		$j = [ 0, [ 18, number - 12 ] ];
	} else {
		$j = [ 1 ];
	}
	return $j;
}
function decode_csi(bytes2, at, resolving, ended) {
	const length = bytes2.length;
	let index = at + 2;
	while (index < length && bytes2.at(index) >= 48 && bytes2.at(index) <= 63) {
		index = index + 1;
	}
	const parameters_end = index;
	while (index < length && bytes2.at(index) >= 32 && bytes2.at(index) <= 47) {
		index = index + 1;
	}
	if (index >= length) {
		if (!(resolving)) {
			let $e = null;
			if (index === at + 2) {
				$e = [ 2 ];
			} else {
				$e = [ 3 ];
			}
			return $e;
		}
		let $f = null;
		if (index === at + 2) {
			$f = [ 0, [ [ 0, "[" ], false, true, false ], 2 ];
		} else {
			$f = [ 1, index - at ];
		}
		return $f;
	}
	const final = bytes2.at(index);
	if (final < 64 || final > 126) {
		return [ 1, index - at ];
	}
	const used = index + 1 - at;
	let first = 0;
	let second = 0;
	let count = 0;
	let private2 = false;
	let cursor = at + 2;
	while (cursor < parameters_end) {
		const byte = bytes2.at(cursor);
		let $g = null;
		if (byte >= 48 && byte <= 57) {
			if (count === 0) {
				count = 1;
			}
			if (count === 1) {
				first = first * 10 + (byte - 48);
			} else if (count === 2) {
				second = second * 10 + (byte - 48);
			}
			$g = undefined;
		} else if (byte === 59) {
			let $h = null;
			if (count === 0) {
				$h = 2;
			} else {
				$h = count + 1;
			}
			count = $h;
		} else {
			private2 = true;
		}
		$g;
		cursor = cursor + 1;
	}
	if (private2 || parameters_end !== index) {
		return [ 1, used ];
	}
	if (final === 126) {
		if (first === 200) {
			return decode_paste(bytes2, at, at + used, ended);
		}
		const $k = tilde_key(first);
		let $l = null;
		if ($k[0] === 0) {
			const key2 = $k[1];
			$l = [ 0, modified(key2, second), used ];
		} else {
			$l = [ 1, used ];
		}
		return $l;
	}
	if (final === 90) {
		return [ 0, [ [ 4 ], false, false, true ], used ];
	}
	const $r = final_key(final);
	let $s = null;
	if ($r[0] === 0) {
		const key3 = $r[1];
		$s = [ 0, modified(key3, second), used ];
	} else {
		$s = [ 1, used ];
	}
	return $s;
}
function modified(key2, parameter) {
	let $m = null;
	if (parameter > 1) {
		$m = parameter - 1;
	} else {
		$m = 0;
	}
	const bits = $m;
	const $n = bits % 2 === 1;
	const $o = Math.trunc(bits / 2) % 2 === 1 || Math.trunc(bits / 8) % 2 === 1;
	const $p = Math.trunc(bits / 4) % 2 === 1;
	return [ key2, $p, $o, $n ];
}
function decode_paste(bytes2, at, from, ended) {
	const length = bytes2.length;
	let index = from;
	while (index + 6 <= length) {
		if (bytes2.at(index) === 27 && bytes2.at(index + 1) === 91 && bytes2.at(index + 2) === 50 && bytes2.at(index + 3) === 48 && bytes2.at(index + 4) === 49 && bytes2.at(index + 5) === 126) {
			const text = decode_utf8(bytes2.slice(from, index));
			return [ 0, plain([ 19, text ]), index + 6 - at ];
		}
		index = index + 1;
	}
	let $i = null;
	if (ended) {
		$i = [ 0, plain([ 19, decode_utf8(bytes2.slice(from, length)) ]), length - at ];
	} else {
		$i = [ 3 ];
	}
	return $i;
}
function debug2(self) {
	const $O = self;
	let $P = null;
	if ($O[0] === 0) {
		const p0 = $O[1];
		$P = "Key::Char(" + debug(p0) + ")";
	} else if ($O[0] === 1) {
		const p02 = $O[1];
		$P = "Key::Ctrl(" + debug(p02) + ")";
	} else if ($O[0] === 2) {
		const p03 = $O[1];
		$P = "Key::Alt(" + debug(p03) + ")";
	} else if ($O[0] === 3) {
		$P = "Key::Enter";
	} else if ($O[0] === 4) {
		$P = "Key::Tab";
	} else if ($O[0] === 5) {
		$P = "Key::BackTab";
	} else if ($O[0] === 6) {
		$P = "Key::Backspace";
	} else if ($O[0] === 7) {
		$P = "Key::Delete";
	} else if ($O[0] === 8) {
		$P = "Key::Insert";
	} else if ($O[0] === 9) {
		$P = "Key::Escape";
	} else if ($O[0] === 10) {
		$P = "Key::Up";
	} else if ($O[0] === 11) {
		$P = "Key::Down";
	} else if ($O[0] === 12) {
		$P = "Key::Left";
	} else if ($O[0] === 13) {
		$P = "Key::Right";
	} else if ($O[0] === 14) {
		$P = "Key::Home";
	} else if ($O[0] === 15) {
		$P = "Key::End";
	} else if ($O[0] === 16) {
		$P = "Key::PageUp";
	} else if ($O[0] === 17) {
		$P = "Key::PageDown";
	} else if ($O[0] === 18) {
		const p04 = $O[1];
		$P = "Key::F(" + JSON.stringify(p04) + ")";
	} else {
		const p05 = $O[1];
		$P = "Key::Paste(" + debug(p05) + ")";
	}
	return $P;
}
function is_empty(self) {
	return self.length === 0;
}
function bytes(codes) {
	if (is_empty(codes)) {
		return new Uint8Array(0);
	}
	const values = codes.split(" ");
	const out = new Uint8Array(values.length);
	let index = 0;
	for (const value of values) {
		set(out, index, unwrap(__parse_i32(value), "key-decoder.vl:25:36"));
		index = index + 1;
	}
	return out;
}
function describe(press) {
	let text = debug2(key(press));
	if (press[1]) {
		text = text + " +ctrl";
	}
	if (press[2]) {
		text = text + " +alt";
	}
	if (press[3]) {
		text = text + " +shift";
	}
	return text;
}
function drain(decoder, chunk) {
	while (true) {
		const $I = next(decoder);
		let $J = null;
		if ($I[0] === 0) {
			const press = $I[1];
			$J = console.log("  " + chunk + ": " + describe(press));
		} else {
			break;
		}
		$J;
	}
}
function decode(label, chunks) {
	console.log(label);
	let decoder = new2();
	let index = 0;
	for (const chunk of chunks) {
		index = index + 1;
		feed(decoder, bytes(chunk));
		drain(decoder, "chunk " + index);
	}
	finish(decoder);
	drain(decoder, "end");
}
function unwrap(self, caller) {
	const $a = self;
	let $b = null;
	if ($a[0] === 0) {
		const x = __clone($a[1]);
		$b = x;
	} else {
		$b = (() => {
			throw __panic("expected Some but got None", caller);
		})();
	}
	return $b;
}
decode("arrows CSI", [ "27 91 65 27 91 66 27 91 67 27 91 68" ]);
decode("arrows SS3", [ "27 79 65 27 79 66 27 79 67 27 79 68" ]);
decode("home: CSI H, SS3 H, 1~, 7~", [ "27 91 72 27 79 72 27 91 49 126 27 91 55 126" ]);
decode("end: CSI F, SS3 F, 4~, 8~", [ "27 91 70 27 79 70 27 91 52 126 27 91 56 126" ]);
decode("insert delete pageup pagedown", [ "27 91 50 126 27 91 51 126 27 91 53 126 27 91 54 126" ]);
decode("F1-F4 SS3", [ "27 79 80 27 79 81 27 79 82 27 79 83" ]);
decode("F1-F5 CSI ~", [ "27 91 49 49 126 27 91 49 50 126 27 91 49 51 126 27 91 49 52 126 27 91 49 53 126" ]);
decode("F6-F12 CSI ~", [ "27 91 49 55 126 27 91 49 56 126 27 91 49 57 126 27 91 50 48 126 27 91 50 49 126 27 91 50 51 126 27 91 50 52 126" ]);
decode("modified: ctrl+Right shift+Up alt+Left ctrl+Delete shift+F1", [ "27 91 49 59 53 67 27 91 49 59 50 65 27 91 49 59 51 68 27 91 51 59 53 126 27 91 49 59 50 80" ]);
decode("Enter: cr", [ "13" ]);
decode("Enter: lf", [ "10" ]);
decode("Enter: crlf is one", [ "13 10" ]);
decode("Enter: cr cr is two", [ "13 13" ]);
decode("Enter: lf cr is two", [ "10 13" ]);
decode("Enter: lf lf is two", [ "10 10" ]);
decode("Backspace 0x7f and 0x08, Tab, BackTab", [ "127 8 9 27 91 90" ]);
decode("Ctrl: c d z, NUL, 0x1c", [ "3 4 26 0 28" ]);
decode("Alt: x, X, Backspace, Ctrl-c, e-acute", [ "27 120 27 88 27 127 27 3 27 195 169" ]);
decode("UTF-8: 2, 3 and 4 bytes", [ "195 169 226 156 147 240 159 152 128" ]);
decode("printable ASCII", [ "32 126 48 57 65 122" ]);
decode("lone ESC at the end", [ "27" ]);
decode("ESC ESC", [ "27 27" ]);
decode("ESC [ at the end is Alt-[", [ "27 91" ]);
decode("ESC O at the end is Alt-O", [ "27 79" ]);
decode("paste with ESC and line endings inside", [ "27 91 50 48 48 126 97 27 91 66 98 13 10 99 27 91 50 48 49 126 122" ]);
decode("paste never closed", [ "27 91 50 48 48 126 97 98 99" ]);
decode("dropped: unknown final, private marker, unknown SS3, stray paste end", [ "27 91 49 59 57 88 27 91 63 50 53 104 27 79 120 27 91 50 48 49 126 107" ]);
decode("dropped: CSI cut by a control byte", [ "27 91 49 7" ]);
decode("dropped: CSI cut by the end", [ "27 91 49 50" ]);
decode("dropped: stray continuation, cut scalar, 0xff", [ "128 195 40 255 106" ]);
decode("dropped: scalar cut by the end", [ "226 156" ]);
decode("ESC | [ | A: one Up, nothing early (no timeout on a pipe)", [ "27", "91", "65" ]);
decode("ESC | x: Alt-x, the bytes decide", [ "27", "120" ]);
decode("cr | lf: one Enter, at the cr", [ "13", "10" ]);
decode("4-byte scalar one byte at a time", [ "240", "159", "152", "128" ]);
decode("paste with its end split", [ "27 91 50 48 48 126 97 98", "27 91 50 48", "49 126" ]);
decode("ESC, then silence, then [A: the timeout resolves Escape", [ "27", "", "91 65" ]);
decode("ESC [ 1 ; 5 split mid-parameters", [ "27 91 49", "59 53", "67" ]);
//# sourceMappingURL=key-decoder.mjs.map
