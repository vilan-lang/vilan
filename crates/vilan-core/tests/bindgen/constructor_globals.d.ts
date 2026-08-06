// The constructor idiom (E37(a)): TypeScript states a host class as an
// `interface` for the instance side plus a `declare var` whose object type
// carries the construct signature for the static side. Every variant, in one
// file: a bare `new`, `new` with optional parameters, an overload set, statics
// beside `new`, an aliased host symbol, a generic construct signature — and the
// three near-misses that must NOT be read as constructors.
interface Widget {
    readonly id: string;
    press(): void;
}

declare var Widget: {
    prototype: Widget;
    new(): Widget;
};

interface Reply {
    json(): string;
}

declare var Reply: {
    prototype: Reply;
    new(body: string, status?: number): Reply;
    new(): Reply;
    json(data: string): Reply;
    readonly kind: string;
};

interface Picture {
    src: string;
}

declare var Picture: {
    prototype: Picture;
    new(): Picture;
};

// The same idiom under an aliased host symbol.
declare var Frame: {
    new(width?: number): Picture;
};

interface Stream<R> {
    read(): R;
}

declare var Stream: {
    prototype: Stream<any>;
    new <R>(): Stream<R>;
};

// A namespace object, not a class: no construct signature, so no type for its
// members to hang off.
declare var Filters: {
    readonly SHOW_ALL: number;
};

// A plain configuration object: the idiom's syntax, none of its meaning.
declare var config: {
    debug: boolean;
};

// The named-constructor-interface spelling, which bindgen names and refuses.
interface GadgetConstructor {
    new(): Widget;
}

declare var Gadget: GadgetConstructor;
