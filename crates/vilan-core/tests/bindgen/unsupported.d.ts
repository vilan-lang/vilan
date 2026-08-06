// Everything v1 recognizes but deliberately does not map (§5, §3.11).
declare namespace Legacy {
    function helper(): void;
}

declare module "some-package" {
    interface Extra {
        value: string;
    }
}

declare global {
    interface Window {
        extra: string;
    }
}

declare enum Colour {
    Red,
    Green,
}

declare const document: Document;
declare var version: string;

type Conditional<T> = T extends string ? number : boolean;
type Mapped<T> = { [K in keyof T]: string };
type Keys = keyof Options;
type Template = `on${string}`;

interface Awkward {
    tag: symbol;
    both: { a: string } & { b: number };
    callback: Function;
    partial: Partial<Options>;
    (value: string): void;
    [Symbol.iterator](): void;
    private secret: string;
}
