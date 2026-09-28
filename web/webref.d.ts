// The subset of these untyped packages used by our generators.
// Keep in sync with the pinned @webref package APIs.
declare module "@webref/elements" {
  export function listAll(): Promise<Record<string, {
    elements: { name: string; obsolete?: boolean }[];
  }>>;
}

declare module "@webref/css" {
  export function listAll(): Promise<{ properties: { name: string }[] }>;
}

declare module "@webref/idl" {
  // WebIDL2 returns an AST; the generator declares the subset it consumes.
  export function parseAll(): Promise<Record<string, unknown[]>>;
}
