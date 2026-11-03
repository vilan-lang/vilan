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
function __shared_new(value) {
	return { v: value };
}
function __stdin_bytes(signal, wait) {
	return __stdin_wait(signal, true, wait);
}
const __vilan_stdin = { buffer: Buffer.alloc(0), ended: false, carriage: false, started: false, waiters: [] };
function __stdin_start() {
	const state = __vilan_stdin;
	if (state.started) return;
	state.started = true;
	process.stdin.on("data", (chunk) => {
		state.buffer = state.buffer.length === 0 ? chunk : Buffer.concat([ state.buffer, chunk ]);
		__stdin_pump();
	});
	const ended = () => {
		state.ended = true;
		__stdin_pump();
	};
	process.stdin.on("end", ended);
	process.stdin.on("error", ended);
}
function __stdin_pump() {
	const state = __vilan_stdin;
	while (state.waiters.length > 0) {
		if (state.carriage && state.buffer.length > 0) {
			if (state.buffer[0] === 10) state.buffer = state.buffer.subarray(1);
			state.carriage = false;
			continue;
		}
		const waiter = state.waiters[0];
		if (waiter.bytes) {
			if (state.buffer.length > 0) {
				const chunk = new Uint8Array(state.buffer);
				state.buffer = Buffer.alloc(0);
				state.waiters.shift().resolve([ 0, chunk ]);
				continue;
			}
			if (state.ended) {
				state.waiters.shift().resolve([ 1 ]);
				continue;
			}
		} else {
			const end = state.buffer.findIndex((byte) => byte === 10 || byte === 13);
			if (end >= 0) {
				const line = state.buffer.toString("utf8", 0, end);
				state.carriage = state.buffer[end] === 13;
				state.buffer = state.buffer.subarray(end + 1);
				state.waiters.shift().resolve([ 0, line ]);
				continue;
			}
			if (state.ended) {
				const rest = state.buffer;
				state.buffer = Buffer.alloc(0);
				state.waiters.shift().resolve(rest.length > 0 ? [ 0, rest.toString("utf8") ] : [ 1 ]);
				continue;
			}
		}
		process.stdin.ref?.();
		process.stdin.resume();
		return;
	}
	if (state.started && !state.ended) {
		process.stdin.pause();
		process.stdin.unref?.();
	}
}
function __stdin_wait(signal, bytes, wait) {
	const sig = signal && signal[0] === 0 ? signal[1] : undefined;
	return new Promise((resolve, reject) => {
		if (sig && sig.aborted) {
			reject(sig.reason);
			return;
		}
		const state = __vilan_stdin;
		__stdin_start();
		let timer;
		const withdraw = () => {
			const parked = state.waiters.indexOf(waiter);
			if (parked >= 0) state.waiters.splice(parked, 1);
			if (sig) sig.removeEventListener("abort", abort);
			if (timer !== undefined) clearTimeout(timer);
		};
		const abort = () => {
			withdraw();
			reject(sig.reason);
			__stdin_pump();
		};
		const waiter = { bytes, resolve: (value) => {
			withdraw();
			resolve(value);
		} };
		state.waiters.push(waiter);
		if (sig) sig.addEventListener("abort", abort, { once: true });
		if (wait >= 0) timer = setTimeout(() => {
			withdraw();
			resolve([ 0, new Uint8Array(0) ]);
			__stdin_pump();
		}, wait);
		__stdin_pump();
	});
}
function __substring(text, start, end) {
	if (0 <= start && start <= end && end <= text.length) return text.substring(start, end);
	throw "substring out of range: the length is " + text.length + " but the range is " + start + ".." + end + " — substring requires 0 <= start <= end <= len and never clamps or swaps; to drop a known affix use strip_prefix/strip_suffix, and for the rest of the string pass s.len() as the end";
}
let __vilan_keys = null;
function __term_enter() {
	if (__vilan_keys !== null) return -1;
	if (!process.stdin.isTTY) {
		__vilan_keys = { terminal: false };
		return 0;
	}
	const keys = { terminal: true, paste: process.stdout.isTTY === true, exit: () => __term_leave() };
	const raise = (name) => () => {
		__term_leave();
		process.kill(process.pid, name);
	};
	keys.term = raise("SIGTERM");
	keys.hup = raise("SIGHUP");
	keys.int = raise("SIGINT");
	__vilan_keys = keys;
	process.stdin.setRawMode(true);
	if (keys.paste) process.stdout.write("\x1b[?2004h");
	process.on("exit", keys.exit);
	process.on("SIGTERM", keys.term);
	process.on("SIGHUP", keys.hup);
	process.on("SIGINT", keys.int);
	return 1;
}
function __term_leave() {
	const keys = __vilan_keys;
	if (keys === null) return;
	__vilan_keys = null;
	if (!keys.terminal) return;
	process.stdin.setRawMode(false);
	if (keys.paste) process.stdout.write("\x1b[?2004l");
	process.removeListener("exit", keys.exit);
	process.removeListener("SIGTERM", keys.term);
	process.removeListener("SIGHUP", keys.hup);
	process.removeListener("SIGINT", keys.int);
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
function decode_utf8(bytes) {
	return new TextDecoder().decode(bytes);
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
			let $U = null;
			if (unit === 92) {
				$U = "\\\\";
			} else if (unit === 34) {
				$U = "\\\"";
			} else if (unit === 10) {
				$U = "\\n";
			} else if (unit === 9) {
				$U = "\\t";
			} else if (unit === 13) {
				$U = "\\r";
			} else if (unit === 0) {
				$U = "\\0";
			} else if (high === 0) {
				$U = "\\u{" + __substring(digits, low, low + 1) + "}";
			} else {
				$U = "\\u{" + __substring(digits, high, high + 1) + __substring(digits, low, low + 1) + "}";
			}
			const escaped = $U;
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
function key(self) {
	const $P = self[0];
	let $Q = null;
	if ($P[0] === 0) {
		const scalar = $P[1];
		let $R = null;
		if (self[1]) {
			$R = [ 1, scalar ];
		} else if (self[2]) {
			$R = [ 2, scalar ];
		} else {
			$R = [ 0, scalar ];
		}
		$Q = $R;
	} else if ($P[0] === 4) {
		let $S = null;
		if (self[3]) {
			$S = [ 5 ];
		} else {
			$S = [ 4 ];
		}
		$Q = $S;
	} else {
		const other = __clone($P);
		$Q = other;
	}
	return $Q;
}
function plain(key2) {
	return [ __clone(key2), false, false, false ];
}
function keys() {
	const opened = __term_enter();
	if (opened < 0) {
		(() => {
			throw __panic("process::keys(): a `Keys` is already live \u{2014} two owners of one terminal mode cannot both restore it; drop the first before opening another", "std/src/process.vl:192:3");
		})();
	}
	return [ __shared_new(new2()), opened === 1 ];
}
async function next(self, $V) {
	const $W = await (next_press(self, $V));
	let $X = null;
	if ($W[0] === 0) {
		const press = $W[1];
		$X = [ 0, key(press) ];
	} else {
		$X = [ 1 ];
	}
	return $X;
}
async function next_press(self, $a) {
	while (true) {
		const $G = step(self[0].v, self[1]);
		let $H = null;
		if ($G[0] === 0) {
			const press = $G[1];
			return [ 0, press ];
		} else if ($G[0] === 1) {
			return [ 1 ];
		} else {
			const wait = $G[1];
			$H = wait;
		}
		const wait2 = $H;
		const $L = await (__stdin_bytes(ambient_signal($a), wait2));
		let $M = null;
		if ($L[0] === 0) {
			const chunk = $L[1];
			$M = feed(self[0].v, chunk);
		} else {
			$M = finish(self[0].v);
		}
		$M;
	}
}
function drop(self) {
	__term_leave();
}
function new2() {
	return [ new Uint8Array(0), 0, false, false, false ];
}
function feed(self, bytes) {
	if (bytes.length === 0) {
		self[3] = true;
	} else {
		const rest = self[0].slice(self[1], self[0].length);
		self[0] = concat(rest, bytes);
		self[1] = 0;
	}
}
function finish(self) {
	self[4] = true;
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
			let $b = null;
			if (self[4]) {
				$b = [ 1 ];
			} else {
				$b = [ 2, -(1) ];
			}
			return $b;
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
function decode_step(bytes, at, resolving, ended) {
	const lead = bytes.at(at);
	let $z = null;
	if (lead === 27) {
		$z = decode_escape(bytes, at, resolving, ended);
	} else if (lead >= 128) {
		$z = decode_scalar(bytes, at, ended);
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
function decode_scalar(bytes, at, ended) {
	const lead = bytes.at(at);
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
	const left = bytes.length - at;
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
		const continuation = bytes.at(at + index);
		if (continuation < 128 || continuation > 191) {
			return [ 1, index ];
		}
		index = index + 1;
	}
	const scalar = decode_utf8(bytes.slice(at, at + width));
	return [ 0, plain([ 0, scalar ]), width ];
}
function decode_escape(bytes, at, resolving, ended) {
	const left = bytes.length - at;
	if (left === 1) {
		let $c = null;
		if (resolving) {
			$c = [ 0, plain([ 9 ]), 1 ];
		} else {
			$c = [ 2 ];
		}
		return $c;
	}
	const second = bytes.at(at + 1);
	let $t = null;
	if (second === 91) {
		$t = decode_csi(bytes, at, resolving, ended);
	} else if (second === 79) {
		$t = decode_ss3(bytes, at, resolving);
	} else if (second === 27) {
		$t = [ 0, plain([ 9 ]), 1 ];
	} else {
		const $x = decode_step(bytes, at + 1, resolving, ended);
		let $y = null;
		if ($x[0] === 0) {
			const press = $x[1];
			const used = $x[2];
			$y = [ 0, [ __clone(press[0]), press[1], true, press[3] ], used + 1 ];
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
function decode_ss3(bytes, at, resolving) {
	if (bytes.length - at < 3) {
		let $u = null;
		if (resolving) {
			$u = [ 0, [ [ 0, "O" ], false, true, true ], 2 ];
		} else {
			$u = [ 2 ];
		}
		return $u;
	}
	const final = bytes.at(at + 2);
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
	let $i = null;
	if (number === 1 || number === 7) {
		$i = [ 0, [ 14 ] ];
	} else if (number === 2) {
		$i = [ 0, [ 8 ] ];
	} else if (number === 3) {
		$i = [ 0, [ 7 ] ];
	} else if (number === 4 || number === 8) {
		$i = [ 0, [ 15 ] ];
	} else if (number === 5) {
		$i = [ 0, [ 16 ] ];
	} else if (number === 6) {
		$i = [ 0, [ 17 ] ];
	} else if (number >= 11 && number <= 15) {
		$i = [ 0, [ 18, number - 10 ] ];
	} else if (number >= 17 && number <= 21) {
		$i = [ 0, [ 18, number - 11 ] ];
	} else if (number === 23 || number === 24) {
		$i = [ 0, [ 18, number - 12 ] ];
	} else {
		$i = [ 1 ];
	}
	return $i;
}
function decode_csi(bytes, at, resolving, ended) {
	const length = bytes.length;
	let index = at + 2;
	while (index < length && bytes.at(index) >= 48 && bytes.at(index) <= 63) {
		index = index + 1;
	}
	const parameters_end = index;
	while (index < length && bytes.at(index) >= 32 && bytes.at(index) <= 47) {
		index = index + 1;
	}
	if (index >= length) {
		if (!(resolving)) {
			let $d = null;
			if (index === at + 2) {
				$d = [ 2 ];
			} else {
				$d = [ 3 ];
			}
			return $d;
		}
		let $e = null;
		if (index === at + 2) {
			$e = [ 0, [ [ 0, "[" ], false, true, false ], 2 ];
		} else {
			$e = [ 1, index - at ];
		}
		return $e;
	}
	const final = bytes.at(index);
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
		const byte = bytes.at(cursor);
		let $f = null;
		if (byte >= 48 && byte <= 57) {
			if (count === 0) {
				count = 1;
			}
			if (count === 1) {
				first = first * 10 + (byte - 48);
			} else if (count === 2) {
				second = second * 10 + (byte - 48);
			}
			$f = undefined;
		} else if (byte === 59) {
			let $g = null;
			if (count === 0) {
				$g = 2;
			} else {
				$g = count + 1;
			}
			count = $g;
		} else {
			private2 = true;
		}
		$f;
		cursor = cursor + 1;
	}
	if (private2 || parameters_end !== index) {
		return [ 1, used ];
	}
	if (final === 126) {
		if (first === 200) {
			return decode_paste(bytes, at, at + used, ended);
		}
		const $j = tilde_key(first);
		let $k = null;
		if ($j[0] === 0) {
			const key2 = $j[1];
			$k = [ 0, modified(key2, second), used ];
		} else {
			$k = [ 1, used ];
		}
		return $k;
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
	let $l = null;
	if (parameter > 1) {
		$l = parameter - 1;
	} else {
		$l = 0;
	}
	const bits = $l;
	const $m = __clone(key2);
	const $n = bits % 2 === 1;
	const $o = Math.trunc(bits / 2) % 2 === 1 || Math.trunc(bits / 8) % 2 === 1;
	const $p = Math.trunc(bits / 4) % 2 === 1;
	return [ $m, $p, $o, $n ];
}
function decode_paste(bytes, at, from, ended) {
	const length = bytes.length;
	let index = from;
	while (index + 6 <= length) {
		if (bytes.at(index) === 27 && bytes.at(index + 1) === 91 && bytes.at(index + 2) === 50 && bytes.at(index + 3) === 48 && bytes.at(index + 4) === 49 && bytes.at(index + 5) === 126) {
			const text = decode_utf8(bytes.slice(from, index));
			return [ 0, plain([ 19, text ]), index + 6 - at ];
		}
		index = index + 1;
	}
	let $h = null;
	if (ended) {
		$h = [ 0, plain([ 19, decode_utf8(bytes.slice(from, length)) ]), length - at ];
	} else {
		$h = [ 3 ];
	}
	return $h;
}
function debug2(self) {
	const $T = self;
	let $U = null;
	if ($T[0] === 0) {
		const p0 = $T[1];
		$U = "Key::Char(" + debug(p0) + ")";
	} else if ($T[0] === 1) {
		const p02 = $T[1];
		$U = "Key::Ctrl(" + debug(p02) + ")";
	} else if ($T[0] === 2) {
		const p03 = $T[1];
		$U = "Key::Alt(" + debug(p03) + ")";
	} else if ($T[0] === 3) {
		$U = "Key::Enter";
	} else if ($T[0] === 4) {
		$U = "Key::Tab";
	} else if ($T[0] === 5) {
		$U = "Key::BackTab";
	} else if ($T[0] === 6) {
		$U = "Key::Backspace";
	} else if ($T[0] === 7) {
		$U = "Key::Delete";
	} else if ($T[0] === 8) {
		$U = "Key::Insert";
	} else if ($T[0] === 9) {
		$U = "Key::Escape";
	} else if ($T[0] === 10) {
		$U = "Key::Up";
	} else if ($T[0] === 11) {
		$U = "Key::Down";
	} else if ($T[0] === 12) {
		$U = "Key::Left";
	} else if ($T[0] === 13) {
		$U = "Key::Right";
	} else if ($T[0] === 14) {
		$U = "Key::Home";
	} else if ($T[0] === 15) {
		$U = "Key::End";
	} else if ($T[0] === 16) {
		$U = "Key::PageUp";
	} else if ($T[0] === 17) {
		$U = "Key::PageDown";
	} else if ($T[0] === 18) {
		const p04 = $T[1];
		$U = "Key::F(" + JSON.stringify(p04) + ")";
	} else {
		const p05 = $T[1];
		$U = "Key::Paste(" + debug(p05) + ")";
	}
	return $U;
}
function debug3(self) {
	return "Press { key = " + debug2(self[0]) + ", " + "ctrl = " + JSON.stringify(self[1]) + ", " + "alt = " + JSON.stringify(self[2]) + ", " + "shift = " + JSON.stringify(self[3]) + " }";
}
function ambient_signal($I) {
	const $J = $I;
	let $K = null;
	if ($J[0] === 0) {
		const n = $J[1];
		$K = [ 0, n.signal_of() ];
	} else {
		$K = [ 1 ];
	}
	return $K;
}
function is_none(self) {
	const $Y = self;
	return $Y[0] === 1;
}
function $Z($aa) {
	drop($aa);
}
(async () => {
	const keys2 = keys();
	try {
		let count = 0;
		while (true) {
			const $N = await (next_press(keys2, [ 1 ]));
			let $O = null;
			if ($N[0] === 0) {
				const press = $N[1];
				count = count + 1;
				console.log("" + count + ": " + debug2(key(press)) + " | " + debug3(press));
				$O = undefined;
			} else {
				break;
			}
			$O;
		}
		console.log("keys: " + count);
		console.log("next past the end is None: " + is_none(await (next(keys2, [ 1 ]))));
	} finally {
		$Z(keys2);
	}
})().catch(($ab) => {
	console.error(String($ab));
	process.exit(1);
});
//# sourceMappingURL=stdin-keys.mjs.map
