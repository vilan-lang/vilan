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
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
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
	parent[2].v.push([ 0, self ]);
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
	const $T = tag;
	let $U = null;
	if ($T === "area") {
		$U = true;
	} else if ($T === "base") {
		$U = true;
	} else if ($T === "br") {
		$U = true;
	} else if ($T === "col") {
		$U = true;
	} else if ($T === "embed") {
		$U = true;
	} else if ($T === "hr") {
		$U = true;
	} else if ($T === "img") {
		$U = true;
	} else if ($T === "input") {
		$U = true;
	} else if ($T === "link") {
		$U = true;
	} else if ($T === "meta") {
		$U = true;
	} else if ($T === "source") {
		$U = true;
	} else if ($T === "track") {
		$U = true;
	} else if ($T === "wbr") {
		$U = true;
	} else {
		$U = false;
	}
	return $U;
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
		const $V = child;
		let $W = null;
		if ($V[0] === 0) {
			const element = $V[1];
			out = out + render(element);
			$W = undefined;
		} else {
			const content = $V[1];
			out = out + escape_text(content);
			$W = undefined;
		}
		$W;
	}
	return out + "</" + view2[0] + ">";
}
function app(title2, todos2, page2) {
	const heading = $g(view("h1"), __clone(title2));
	const list = $o(view("ul"), $n(__clone(todos2), (todo) => {
		return todo;
	}, (todo, $m) => {
		return text(view("li"), todo);
	}));
	const nav = $J(view("nav"), $I(__clone(page2), (current, $E) => {
		const $F = current;
		let $G = null;
		if ($F[0] === 0) {
			$G = text($H(view("a"), "href", "/"), "Home");
		} else {
			$G = text($H(view("a"), "href", "/about"), "About & friends");
		}
		return $G;
	}));
	return $S($S($S($H(view("main"), "id", "app"), heading), list), nav);
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(__clone(value)), __shared_new(subscribers) ];
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
	const instance = $i(flow);
	const value = instance[0]();
	instance[2]();
	return value;
}
function $g(self, source) {
	self[3].v = $h(source);
	self[2].v = [  ];
	return __clone(self);
}
function $n(source, key, render2) {
	return [ source, key, render2 ];
}
function $t(self) {
	return [ 1 ];
}
function $x(self, subscriber) {
	return $l(self, subscriber);
}
function $z(self, cursor) {
	let none = [  ];
	return none;
}
function $A(self, cursor) {

}
function $s(self) {
	const $u = $t(self);
	let $v = null;
	if ($u[0] === 0) {
		const cursor = $u[1];
		$v = [ () => {
			return $j(self);
		}, (subscriber) => {
			return $x(self, subscriber);
		}, () => {
			return $z(self, cursor);
		}, () => {
			return $A(self, cursor);
		} ];
	} else {
		$v = [ () => {
			return $j(self);
		}, (subscriber) => {
			return $x(self, subscriber);
		}, () => {
			return [ [ 2, $j(self) ] ];
		}, () => {
			return;
		} ];
	}
	return $v;
}
function $r(source) {
	const instance = $s(source);
	const items = instance[0]();
	instance[3]();
	return items;
}
function $C(self, content) {
	place(content, self[0]);
	return [  ];
}
function $D(owner, body) {
	return body(owner);
}
function $q(parent, source, key, render2) {
	const region = open(parent);
	const items = $r(source);
	for (const item of items) {
		const owner = new2();
		$D(owner, ($B) => {
			return $C(region, render2(item, $B));
		});
	}
}
function $p(self, parent) {
	$q(parent, __clone(self[0]), self[1], self[2]);
}
function $o(self, content) {
	$p(content, self);
	return __clone(self);
}
function $H(self, name2, value) {
	apply(value, self, name2);
	return __clone(self);
}
function $I(source, render2) {
	return [ source, render2 ];
}
function $N(self) {
	return [ () => {
		return $j(self);
	}, (subscriber) => {
		return $x(self, subscriber);
	}, () => {
		return;
	} ];
}
function $M(flow) {
	const instance = $N(flow);
	const value = instance[0]();
	instance[2]();
	return value;
}
function $L(parent, source, render2) {
	const region = open(parent);
	const value = $M(source);
	const owner = new2();
	$D(owner, ($R) => {
		return $C(region, render2(value, $R));
	});
}
function $K(self, parent) {
	$L(parent, __clone(self[0]), self[1]);
}
function $J(self, content) {
	$K(content, self);
	return __clone(self);
}
function $S(self, content) {
	place(content, self);
	return __clone(self);
}
function $Z(flow, parent, name2) {
	set_attribute(parent[1], name2, $h(flow));
}
function $Y(self, parent, name2) {
	$Z(self, parent, name2);
}
function $X(self, name2, value) {
	$Y(value, self, name2);
	return __clone(self);
}
function $aa(self, content) {
	place2(content, self);
	return __clone(self);
}
function $ad(flow, parent) {
	parent[2].v.push([ 1, $h(flow) ]);
}
function $ac(self, parent) {
	$ad(self, parent);
}
function $ab(self, content) {
	$ac(content, self);
	return __clone(self);
}
function $ae(self, content) {
	place3(content, self);
	return __clone(self);
}
const no_cleanups = __shared_new([  ]);
const title = $a("Tasks <live>");
const todos = $c([ "alpha", "beta & gamma" ]);
const page = $c([ 1 ]);
console.log(render(app(title, todos, page)));
console.log(render(text(view("p"), "<script>alert(\"&\")</script>")));
console.log(render($H($H(view("img"), "src", "/logo.png"), "alt", "a & b")));
console.log(render($S($H(view("svg"), "viewBox", "0 0 24 24"), $H(view("path"), "d", "M5 12h14"))));
const name = $a("world & <you>");
const mixed = $ab($aa($S($aa($X(view("p"), "data-live", $a("a \"quoted\" & value")), "Take "), text(view("code"), "vilan upgrade")), " & enjoy. "), name);
console.log(render(mixed));
const pair = [ text(view("i"), "a"), text(view("b"), "b") ];
console.log(render($ae(view("p"), pair)));
console.log(render(text($aa(view("p"), "gone"), "kept")));
