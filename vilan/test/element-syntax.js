function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __shared_new(value) {
	return { v: value };
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
			updated.push(attribute);
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
function place3(self, parent) {
	parent[2].v.push([ 1, $c(self) ]);
}
function apply(self, parent, name2) {
	set_attribute(parent[1], name2, self);
}
function apply2(self, parent, name2) {
	set_attribute(parent[1], name2, $c(self));
}
function is_void_element(tag) {
	const $h = tag;
	let $i = null;
	if ($h === "area") {
		$i = true;
	} else if ($h === "base") {
		$i = true;
	} else if ($h === "br") {
		$i = true;
	} else if ($h === "col") {
		$i = true;
	} else if ($h === "embed") {
		$i = true;
	} else if ($h === "hr") {
		$i = true;
	} else if ($h === "img") {
		$i = true;
	} else if ($h === "input") {
		$i = true;
	} else if ($h === "link") {
		$i = true;
	} else if ($h === "meta") {
		$i = true;
	} else if ($h === "source") {
		$i = true;
	} else if ($h === "track") {
		$i = true;
	} else if ($h === "wbr") {
		$i = true;
	} else {
		$i = false;
	}
	return $i;
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
		const $j = child;
		let $k = null;
		if ($j[0] === 0) {
			const element = $j[1];
			out = out + render(element);
			$k = undefined;
		} else {
			const content = $j[1];
			out = out + escape_text(content);
			$k = undefined;
		}
		$k;
	}
	return out + "</" + view2[0] + ">";
}
function row(label) {
	return $e($d(view("li"), "class", "item"), label);
}
function $a(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $c(self) {
	return self[0].v;
}
function $b(self, name2, value) {
	apply2(value, self, name2);
	return self;
}
function $d(self, name2, value) {
	apply(value, self, name2);
	return self;
}
function $e(self, content) {
	place2(content, self);
	return self;
}
function $f(self, content) {
	place(content, self);
	return self;
}
function $g(self, content) {
	place3(content, self);
	return self;
}
const name = $a("world & <you>");
console.log(render($g($e($f($e($d($b(view("p"), "data-live", name), "title", "hi"), "Take "), $e(view("code"), "vilan upgrade")), " & enjoy. "), name)));
console.log(render($d($d($d(view("input"), "type", "checkbox"), "disabled", ""), "aria-label", "Done")));
console.log(render($f($f(view("ul"), row("alpha")), row("beta"))));
console.log(render($f($d(view("svg"), "viewBox", "0 0 24 24"), $d(view("path"), "d", "M5 12h14"))));
console.log(render($f(view("div"), $e(view("span"), "chained"))));
