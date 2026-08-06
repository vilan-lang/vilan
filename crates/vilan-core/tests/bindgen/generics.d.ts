// Generics in v1 scope: unbounded parameters, defaults, and bounds.
interface Box<T> {
    value: T;
    map(next: T): Box<T>;
}

interface Pair<K, V> {
    key: K;
    value: V;
}

interface Defaulted<T = string> {
    value: T;
}

interface Bounded<T extends Box<string>> {
    value: T;
}

declare function identity<T>(value: T): T;
