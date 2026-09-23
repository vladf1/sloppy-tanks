/** Small strict JSON readers shared by the host and mirror, loaded only for multiplayer. */
export interface Reader<T> {
  read(value: unknown): T;
}
export function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Expected object");
  }
  return value as Record<string, unknown>;
}
export function number(min = -1e9, max = 1e9, integer = false): Reader<number> {
  return {
    read(value) {
      if (
        typeof value !== "number" ||
        !Number.isFinite(value) ||
        value < min ||
        value > max ||
        (integer && !Number.isSafeInteger(value))
      ) {
        throw new Error("Invalid number");
      }
      return value;
    },
  };
}
export const id = number(0, Number.MAX_SAFE_INTEGER, true);
export const boolean: Reader<boolean> = {
  read(value) {
    if (typeof value !== "boolean") {
      throw new Error("Invalid boolean");
    }
    return value;
  },
};
export function string(max: number, min = 0): Reader<string> {
  return {
    read(value) {
      if (typeof value !== "string" || value.length < min || value.length > max) {
        throw new Error("Invalid text");
      }
      return value;
    },
  };
}
export function enumeration<const T extends readonly (string | number | null)[]>(
  ...values: T
): Reader<T[number]> {
  return {
    read(value) {
      if (!values.some((item) => item === value)) {
        throw new Error("Invalid choice");
      }
      return value as T[number];
    },
  };
}
export function optional<T>(reader: Reader<T>): Reader<T | undefined> {
  return { read: (value) => (value === undefined ? undefined : reader.read(value)) };
}
export function nullable<T>(reader: Reader<T>): Reader<T | null> {
  return { read: (value) => (value === null ? null : reader.read(value)) };
}
export function array<T>(reader: Reader<T>, max: number): Reader<T[]> {
  return {
    read(value) {
      if (!Array.isArray(value) || value.length > max) {
        throw new Error("Invalid list");
      }
      return value.map((item) => reader.read(item));
    },
  };
}
/** Copies the declared fields only; no physics objects or unknown nested properties pass through. */
export function object<T extends object>(shape: { [K in keyof T]-?: Reader<T[K]> }): Reader<T> {
  return {
    read(value) {
      const source = record(value);
      const out = {} as T;
      for (const key of Object.keys(shape) as (keyof T)[]) {
        let item: T[keyof T];
        try {
          item = shape[key].read(source[key as string]);
        } catch (error) {
          throw new Error(
            String(key) + ": " + (error instanceof Error ? error.message : "Invalid field"),
            { cause: error },
          );
        }
        if (item !== undefined) {
          out[key] = item;
        }
      }
      return out;
    },
  };
}
