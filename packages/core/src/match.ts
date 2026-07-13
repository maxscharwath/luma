// A tiny, typed pattern-matcher to replace `if/else-if` and ternary ladders with
// a flat, declarative chain. First match wins; predicates and results are lazy
// (only the winning branch's value function runs). Shared by every client.
//
//   const label = match(video)
//     .when((v) => v.hdr, 'HDR')
//     .when((v) => v.h265, 'H.265')
//     .otherwise(null);
//
//   // value equality (no predicate needed):
//   const icon = match(status)
//     .when('ready', '✓')
//     .when('error', '✕')
//     .otherwise('…');

type Predicate<T> = (value: T) => boolean;
/** A branch result: a literal, or a lazy producer evaluated only if it wins. */
type Produce<T, R> = R | ((value: T) => R);

function evaluate<T, R>(produce: Produce<T, R>, value: T): R {
  return typeof produce === 'function' ? (produce as (value: T) => R)(value) : produce;
}

class Matcher<T, R = never> {
  private done = false;
  private result: unknown;

  constructor(private readonly value: T) {}

  /** Add a branch. `cond` is a predicate, or a value compared with `===`. The
   * accumulated result type widens with each branch's produced type. */
  when<U>(cond: Predicate<T> | T, produce: Produce<T, U>): Matcher<T, R | U> {
    if (!this.done) {
      const hit =
        typeof cond === 'function' ? (cond as Predicate<T>)(this.value) : this.value === cond;
      if (hit) {
        this.done = true;
        this.result = evaluate(produce, this.value);
      }
    }
    return this as unknown as Matcher<T, R | U>;
  }

  /** Resolve: the winning branch's value, else the fallback. */
  otherwise<U>(fallback: Produce<T, U>): R | U {
    return (this.done ? this.result : evaluate(fallback, this.value)) as R | U;
  }
}

/** Start a match chain over `value`. See the examples above. */
export function match<T>(value: T): Matcher<T> {
  return new Matcher<T>(value);
}
