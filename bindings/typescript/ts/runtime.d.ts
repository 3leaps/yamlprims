// SPDX-License-Identifier: MIT OR Apache-2.0
// Minimal runtime globals used by @3leaps/yamlprims (Node.js, Deno, Bun, browsers).
// Declaring them directly keeps the build free of DOM or Node type packages.
declare namespace WebAssembly {
  interface Memory {
    readonly buffer: ArrayBuffer;
  }
  class Module {
    constructor(bytes: Uint8Array);
  }
  class Instance {
    constructor(module: Module, imports: object);
    readonly exports: object;
  }
  class RuntimeError extends Error {}
}
declare class TextEncoder {
  encode(input: string): Uint8Array;
}
declare class TextDecoder {
  constructor(label: string, options: { fatal: boolean });
  decode(input: Uint8Array): string;
}
declare function atob(data: string): string;
