function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __shared_new(value) {
	return { v: value };
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function new2() {
	return [ __shared_new([ 0, [ 1 ], [ 1 ] ]), 0 ];
}
function view(tag) {
	const attributes = __shared_new([  ]);
	if (tag === "svg") {
		set_attribute(attributes, "xmlns", "http://www.w3.org/2000/svg");
	}
	return [ tag, attributes, __shared_new([  ]), __shared_new("") ];
}
function set_attribute(attributes, name2, value) {
	let updated = [  ];
	let found = false;
	for (const attribute of attributes.v) {
		if (attribute[0] === name2) {
			updated.push([ name2, value ]);
			found = true;
		} else {
			updated.push(__clone(attribute));
		}
	}
	if (!(found)) {
		updated.push([ name2, value ]);
	}
	attributes.v = updated;
}
function text(self, content) {
	self[3].v = content;
	self[2].v = [  ];
	return __clone(self);
}
function open(parent) {
	return [ __clone(parent) ];
}
function place(self, parent) {
	parent[2].v.push([ 0, __clone(self) ]);
}
function place2(self, parent) {
	parent[2].v.push([ 1, self ]);
}
function place3(self, parent) {
	for (const item of self) {
		parent[2].v.push([ 0, __clone(item) ]);
	}
}
function apply(self, parent, name2) {
	set_attribute(parent[1], name2, self);
}
function is_void_element(tag) {
	const $K = tag;
	let $L = null;
	if ($K === "area") {
		$L = true;
	} else if ($K === "base") {
		$L = true;
	} else if ($K === "br") {
		$L = true;
	} else if ($K === "col") {
		$L = true;
	} else if ($K === "embed") {
		$L = true;
	} else if ($K === "hr") {
		$L = true;
	} else if ($K === "img") {
		$L = true;
	} else if ($K === "input") {
		$L = true;
	} else if ($K === "link") {
		$L = true;
	} else if ($K === "meta") {
		$L = true;
	} else if ($K === "source") {
		$L = true;
	} else if ($K === "track") {
		$L = true;
	} else if ($K === "wbr") {
		$L = true;
	} else {
		$L = false;
	}
	return $L;
}
function escape_text(value) {
	return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}
function escape_attribute(value) {
	return value.replaceAll("&", "&amp;").replaceAll("\"", "&quot;");
}
function render(view2) {
	let out = "<" + view2[0];
	for (const attribute of view2[1].v) {
		out = out + " " + attribute[0] + "=\"" + escape_attribute(attribute[1]) + "\"";
	}
	out = out + ">";
	if (is_void_element(view2[0])) {
		return out;
	}
	out = out + escape_text(view2[3].v);
	for (const child of view2[2].v) {
		const $M = child;
		let $N = null;
		if ($M[0] === 0) {
			const element = $M[1];
			out = out + render(element);
			$N = undefined;
		} else {
			const content = $M[1];
			out = out + escape_text(content);
			$N = undefined;
		}
		$N;
	}
	return out + "</" + view2[0] + ">";
}
function app(title2, todos2, page2) {
	const heading = $g(view("h1"), __clone(title2));
	const list = $o(view("ul"), $n(todos2, (todo) => {
		return todo;
	}, (todo, $m) => {
		return text(view("li"), todo);
	}));
	const nav = $A(view("nav"), $z(__clone(page2), (current, $v) => {
		const $w = current;
		let $x = null;
		if ($w[0] === 0) {
			$x = text($y(view("a"), "href", "/"), "Home");
		} else {
			$x = text($y(view("a"), "href", "/about"), "About & friends");
		}
		return $x;
	}));
	return $J($J($J($y(view("main"), "id", "app"), heading), list), nav);
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $c(value) {
	return $b(value);
}
function $j(self) {
	return __clone(self[0].v);
}
function $l(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $k(self, subscriber) {
	return $l(self, subscriber);
}
function $i(self) {
	return [ () => {
		return $j(self);
	}, (subscriber) => {
		return $k(self, subscriber);
	}, () => {
		return;
	} ];
}
function $h(flow) {
	const instance = $i(__clone(flow));
	const value = instance[0]();
	instance[2]();
	return value;
}
function $g(self, source) {
	self[3].v = $h(__clone(source));
	self[2].v = [  ];
	return __clone(self);
}
function $n(source, key, render2) {
	return [ __clone(source), key, render2 ];
}
function $t(self, content) {
	place(__clone(content), self[0]);
	return [  ];
}
function $u(owner, body) {
	return body(owner);
}
function $q(parent, source, key, render2) {
	const region = open(parent);
	const items = $j(source);
	for (const item of items) {
		const owner = new2();
		$u(owner, ($s) => {
			return $t(region, render2(item, $s));
		});
	}
}
function $p(self, parent) {
	$q(parent, self[0], self[1], self[2]);
}
function $o(self, content) {
	$p(__clone(content), self);
	return __clone(self);
}
function $y(self, name2, value) {
	apply(__clone(value), self, name2);
	return __clone(self);
}
function $z(source, render2) {
	return [ __clone(source), render2 ];
}
function $G(self, subscriber) {
	return $l(self, subscriber);
}
function $E(self) {
	return [ () => {
		return $j(self);
	}, (subscriber) => {
		return $G(self, subscriber);
	}, () => {
		return;
	} ];
}
function $D(flow) {
	const instance = $E(__clone(flow));
	const value = instance[0]();
	instance[2]();
	return value;
}
function $C(parent, source, render2) {
	const region = open(parent);
	const value = $D(__clone(source));
	const owner = new2();
	$u(owner, ($I) => {
		return $t(region, render2(value, $I));
	});
}
function $B(self, parent) {
	$C(parent, __clone(self[0]), self[1]);
}
function $A(self, content) {
	$B(__clone(content), self);
	return __clone(self);
}
function $J(self, content) {
	place(__clone(content), self);
	return __clone(self);
}
function $Q(flow, parent, name2) {
	set_attribute(parent[1], name2, $h(__clone(flow)));
}
function $P(self, parent, name2) {
	$Q(__clone(self), parent, name2);
}
function $O(self, name2, value) {
	$P(__clone(value), self, name2);
	return __clone(self);
}
function $R(self, content) {
	place2(__clone(content), self);
	return __clone(self);
}
function $U(flow, parent) {
	parent[2].v.push([ 1, $h(__clone(flow)) ]);
}
function $T(self, parent) {
	$U(__clone(self), parent);
}
function $S(self, content) {
	$T(__clone(content), self);
	return __clone(self);
}
function $V(self, content) {
	place3(__clone(content), self);
	return __clone(self);
}
const title = $a("Tasks <live>");
const todos = $c([ "alpha", "beta & gamma" ]);
const page = $c([ 1 ]);
console.log(render(app(title, todos, page)));
console.log(render(text(view("p"), "<script>alert(\"&\")</script>")));
console.log(render($y($y(view("img"), "src", "/logo.png"), "alt", "a & b")));
console.log(render($J($y(view("svg"), "viewBox", "0 0 24 24"), $y(view("path"), "d", "M5 12h14"))));
const name = $a("world & <you>");
const mixed = $S($R($J($R($O(view("p"), "data-live", $a("a \"quoted\" & value")), "Take "), text(view("code"), "vilan upgrade")), " & enjoy. "), name);
console.log(render(mixed));
const pair = [ text(view("i"), "a"), text(view("b"), "b") ];
console.log(render($V(view("p"), pair)));
console.log(render(text($R(view("p"), "gone"), "kept")));
