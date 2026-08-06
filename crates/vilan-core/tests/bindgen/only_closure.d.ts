// The `--only` transitive closure (E37(b)), generated with `--only Root` (see
// `only_closure.only`). Everything named `Reached*` is in the output; everything
// named `Unreached*` is not, and each is unreachable for a different reason.
interface Base {
    inherited(): ReachedByBase;
}

interface Root extends Base {
    field: ReachedByField;
    take(value: ReachedByParameter): ReachedByReturn;
    boxed(): ReachedHolder<ReachedByArgument>;
    handler: (event: ReachedByClosure) => void;
    maybe: ReachedByAbsenceUnion | null;
    // An open union widens to `any` and names neither member.
    widened: UnreachedLeftOfUnion | UnreachedRightOfUnion;
    align(side: ReachedAlign): void;
}

interface ReachedHolder<T> {
    value: T;
}

interface ReachedByBase {
    a: string;
}

interface ReachedByField {
    a: string;
}

interface ReachedByParameter {
    a: string;
}

interface ReachedByReturn {
    a: string;
}

interface ReachedByArgument {
    a: string;
}

interface ReachedByClosure {
    a: string;
}

interface ReachedByAbsenceUnion {
    a: string;
}

// A cycle: the closure has to visit each declaration once, not chase the loop.
interface ReachedCycleLeft {
    right(): ReachedCycleRight;
}

interface ReachedCycleRight {
    left(): ReachedCycleLeft;
}

type ReachedAlign = "start" | "end";

declare var Root: {
    prototype: Root;
    new(seed: ReachedByConstructor): Root;
};

interface ReachedByConstructor {
    cycle(): ReachedCycleLeft;
}

// `extends` points at a BASE, so `Root` does not reach its own subtypes.
interface UnreachedDerived extends Root {
    extra: string;
}

interface UnreachedLeftOfUnion {
    a: string;
}

interface UnreachedRightOfUnion {
    a: string;
}

type UnreachedAlign = "up" | "down";

declare function unreachedFunction(): void;
