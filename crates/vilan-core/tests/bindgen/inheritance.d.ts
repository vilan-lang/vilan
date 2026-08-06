// `extends` chains: vilan has no struct inheritance, so members are flattened.
interface Node {
    id: string;
    remove(): void;
}

interface Element extends Node {
    tag: string;
    click(): void;
}

interface Button extends Element {
    // A derived member SHADOWS the base's, as TS's own override rule says.
    id: number;
    press(): void;
}

interface Boxed<T> {
    value: T;
}

interface StringBox extends Boxed<string> {
    trim(): void;
}

interface Orphan extends SomethingElsewhere {
    own: string;
}
