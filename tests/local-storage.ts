// `localStorage` stand-ins for the page-shell tests (the runner only takes `*.test.ts`).

/** Run `check` with `localStorage` defined by `descriptor` (for example a getter that
 * throws, as in a browser denying access), then restore the original. */
export function withLocalStorage(descriptor: PropertyDescriptor, check: () => void): void {
  const original = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  Object.defineProperty(globalThis, "localStorage", { ...descriptor, configurable: true });
  try {
    check();
  } finally {
    if (original) Object.defineProperty(globalThis, "localStorage", original);
    else Reflect.deleteProperty(globalThis, "localStorage");
  }
}

/** Run `check` with `storage` as `localStorage`. */
export function withStorage(storage: object, check: () => void): void {
  withLocalStorage({ value: storage }, check);
}

/** A storage kept in `values`. */
export function memoryStorage() {
  const values = new Map<string, string>();
  return {
    values,
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => void values.set(key, value),
  };
}
