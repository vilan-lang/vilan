function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __stdin_line(signal) {
	return __stdin_wait(signal, false, -1);
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
async function read_line($b) {
	return await (__stdin_line(ambient_signal($b)));
}
async function scan($a) {
	return unwrap_or(await (read_line($a)), "");
}
function ambient_signal($c) {
	const $d = $c;
	let $e = null;
	if ($d[0] === 0) {
		const n = $d[1];
		$e = [ 0, n.signal_of() ];
	} else {
		$e = [ 1 ];
	}
	return $e;
}
function unwrap_or(self, fallback) {
	const $f = self;
	let $g = null;
	if ($f[0] === 0) {
		const x = __clone($f[1]);
		$g = x;
	} else {
		$g = __clone(fallback);
	}
	return $g;
}
function is_none(self) {
	const $j = self;
	return $j[0] === 1;
}
(async () => {
	console.log("scan: [" + await (scan([ 1 ])) + "]");
	let count = 0;
	while (true) {
		const $h = await (read_line([ 1 ]));
		let $i = null;
		if ($h[0] === 0) {
			const line = $h[1];
			count = count + 1;
			console.log("" + count + ": [" + line + "] " + line.length);
			$i = undefined;
		} else {
			break;
		}
		$i;
	}
	console.log("lines after scan: " + count);
	console.log("read_line past the end is None: " + is_none(await (read_line([ 1 ]))));
	console.log("scan past the end: [" + await (scan([ 1 ])) + "]");
})().catch(($k) => {
	console.error(String($k));
	process.exit(1);
});
//# sourceMappingURL=stdin-lines.mjs.map
