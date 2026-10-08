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
function place(self, parent) {
	parent[2].v.push([ 0, self ]);
}
function place2(self, parent) {
	parent[2].v.push([ 1, self ]);
}
function apply(self, parent, name2) {
	set_attribute(parent[1], name2, self);
}
function is_void_element(tag) {
	const $a = tag;
	let $b = null;
	if ($a === "area") {
		$b = true;
	} else if ($a === "base") {
		$b = true;
	} else if ($a === "br") {
		$b = true;
	} else if ($a === "col") {
		$b = true;
	} else if ($a === "embed") {
		$b = true;
	} else if ($a === "hr") {
		$b = true;
	} else if ($a === "img") {
		$b = true;
	} else if ($a === "input") {
		$b = true;
	} else if ($a === "link") {
		$b = true;
	} else if ($a === "meta") {
		$b = true;
	} else if ($a === "source") {
		$b = true;
	} else if ($a === "track") {
		$b = true;
	} else if ($a === "wbr") {
		$b = true;
	} else {
		$b = false;
	}
	return $b;
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
	for (const child4 of view2[2].v) {
		const $c = child4;
		let $d = null;
		if ($c[0] === 0) {
			const element = $c[1];
			out = out + render(element);
			$d = undefined;
		} else {
			const content = $c[1];
			out = out + escape_text(content);
			$d = undefined;
		}
		$d;
	}
	return out + "</" + view2[0] + ">";
}
function row(label) {
	return child(attr2(view("li"), "class", "item"), label);
}
function new2(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function new3(value) {
	return new2(value);
}
function get(self) {
	return __clone(self[0].v);
}
function attach(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function on_settle(self, subscriber) {
	return attach(self, subscriber);
}
function start(self) {
	return [ () => {
		return get(self);
	}, (subscriber) => {
		return on_settle(self, subscriber);
	}, () => {
		return;
	} ];
}
function read_once(flow) {
	const instance = start(flow);
	const value = instance[0]();
	instance[2]();
	return value;
}
function apply_text_flow(flow, parent, name2) {
	set_attribute(parent[1], name2, read_once(flow));
}
function apply2(self, parent, name2) {
	apply_text_flow(self, parent, name2);
}
function attr(self, name2, value) {
	apply2(value, self, name2);
	return __clone(self);
}
function attr2(self, name2, value) {
	apply(value, self, name2);
	return __clone(self);
}
function child(self, content) {
	place2(content, self);
	return __clone(self);
}
function child2(self, content) {
	place(content, self);
	return __clone(self);
}
function place_text_flow(flow, parent) {
	parent[2].v.push([ 1, read_once(flow) ]);
}
function place3(self, parent) {
	place_text_flow(self, parent);
}
function child3(self, content) {
	place3(content, self);
	return __clone(self);
}
const name = new3("world & <you>");
console.log(render(child3(child(child2(child(attr2(attr(view("p"), "data-live", __clone(name)), "title", "hi"), "Take "), child(view("code"), "vilan upgrade")), " & enjoy. "), name)));
console.log(render(attr2(attr2(attr2(view("input"), "type", "checkbox"), "aria-label", "Done"), "disabled", "")));
console.log(render(child2(child2(view("ul"), row("alpha")), row("beta"))));
console.log(render(child2(attr2(view("svg"), "viewBox", "0 0 24 24"), attr2(view("path"), "d", "M5 12h14"))));
console.log(render(child2(view("div"), child(view("span"), "chained"))));
