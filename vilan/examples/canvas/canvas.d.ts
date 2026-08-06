// canvas.d.ts — the slice of the browser's canvas API this example binds.
//
// Hand-written for `vilan bindgen`, in the exact shape `lib.dom.d.ts` states
// these same types: an `interface` for the INSTANCE side, and a `declare var`
// whose object type carries the `new(…)` construct signature for the STATIC
// side. That split is TypeScript's, not a convention invented here, and
// recognizing it is what lets bindgen emit a constructor at all (E37(a),
// `proposal/bindgen.md` §10.3).
//
// It is deliberately NOT a copy of `lib.dom.d.ts`: that file is 39,429 lines,
// and its element types are one strongly-connected component (`Node` owns a
// `Document`, a `UIEvent` has a `Window`), so the transitive closure of
// `HTMLCanvasElement` there is ~900 declarations however tightly it is
// filtered. See `README.md` for the measurement.
//
// It also carries three declarations the canvas surface cannot reach — the
// audio pair and `MediaError` — so that the `--only` filter in `README.md`'s
// regeneration command has something to leave out.

interface EventTarget {
    addEventListener(type: string, listener: (event: Event) => void): void;
    removeEventListener(type: string, listener: (event: Event) => void): void;
}

interface Event {
    readonly type: string;
    preventDefault(): void;
}

interface MouseEvent extends Event {
    readonly offsetX: number;
    readonly offsetY: number;
    readonly shiftKey: boolean;
}

interface HTMLElement extends EventTarget {
    readonly id: string;
    title: string;
    onclick: ((event: MouseEvent) => void) | null;
}

interface HTMLCanvasElement extends HTMLElement {
    width: number;
    height: number;
    getContext(contextId: "2d"): CanvasRenderingContext2D;
    toDataURL(type?: string, quality?: number): string;
}

interface HTMLImageElement extends HTMLElement {
    src: string;
    readonly complete: boolean;
}

interface CanvasGradient {
    addColorStop(offset: number, color: string): void;
}

interface CanvasRenderingContext2D {
    readonly canvas: HTMLCanvasElement;
    // An open union, as the real API has it: a fill can be a CSS colour or a
    // gradient. vilan has no union type, so this widens to `any` under a TODO —
    // left in deliberately, because a generated file that never shows one is
    // not representative of what generating from a real `.d.ts` looks like.
    fillStyle: string | CanvasGradient;
    strokeStyle: string;
    lineWidth: number;
    font: string;
    clearRect(x: number, y: number, w: number, h: number): void;
    fillRect(x: number, y: number, w: number, h: number): void;
    strokeRect(x: number, y: number, w: number, h: number): void;
    beginPath(): void;
    closePath(): void;
    arc(x: number, y: number, radius: number, startAngle: number, endAngle: number, counterclockwise?: boolean): void;
    moveTo(x: number, y: number): void;
    lineTo(x: number, y: number): void;
    fill(): void;
    stroke(): void;
    fillText(text: string, x: number, y: number): void;
    drawImage(image: HTMLImageElement, dx: number, dy: number): void;
    createLinearGradient(x0: number, y0: number, x1: number, y1: number): CanvasGradient;
}

declare var HTMLCanvasElement: {
    prototype: HTMLCanvasElement;
    new(): HTMLCanvasElement;
};

declare var HTMLImageElement: {
    prototype: HTMLImageElement;
    new(): HTMLImageElement;
};

// The same idiom under an aliased host symbol: `new Image(…)` is how a page
// actually builds one, and it yields an `HTMLImageElement`, not an `Image`.
declare var Image: {
    new(width?: number, height?: number): HTMLImageElement;
};

declare var CanvasGradient: {
    prototype: CanvasGradient;
    new(): CanvasGradient;
};

declare var CanvasRenderingContext2D: {
    prototype: CanvasRenderingContext2D;
    new(): CanvasRenderingContext2D;
};

// Unreachable from `HTMLCanvasElement`: nothing in the canvas surface names an
// audio element, and `extends` points at a BASE, so `HTMLElement` does not
// reach its own subtypes. `--only HTMLCanvasElement` drops all three.

interface MediaError {
    readonly code: number;
}

interface HTMLAudioElement extends HTMLElement {
    src: string;
    readonly error: MediaError | null;
    play(): void;
}

declare var Audio: {
    new(src?: string): HTMLAudioElement;
};
