// The three shapes that go by "union", plus absence.
type Align = "start" | "end" | "center";
type Shape =
    | { kind: "circle"; radius: number }
    | { kind: "square"; side: number };

interface Chart {
    align: Align;
    setAlign(align: Align): void;
    caption: string | null;
    subtitle: string | undefined;
    either: string | number;
    shape: Shape;
    inline(align: "left" | "right"): void;
    maybeAlign(align?: Align): void;
    getAlign(): Align;
}
