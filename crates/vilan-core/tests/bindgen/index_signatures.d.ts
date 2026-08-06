// Index signatures: the two owner-flagged rows, plus the mixed shape.
interface Lookup {
    [key: string]: number;
}

interface ArrayLike {
    [index: number]: string;
}

interface Mixed {
    length: number;
    [index: number]: string;
}

interface Records {
    byName: Record<string, number>;
}
