// Interfaces: properties (readable/writable/optional/readonly), methods, and an
// inline object type that has to be given a synthesized name.
interface Options {
    readonly id: string;
    title: string;
    open?: boolean;
    tags: string[];
    origin: { x: number; y: number };
    apply(value: string): void;
    measure(): number;
    get computed(): string;
    set computed(value: string);
}
