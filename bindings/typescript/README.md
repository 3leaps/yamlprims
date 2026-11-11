# @3leaps/yamlprims

YAML 1.2.2 Core Schema value loader, strict wire decoder and canonical encoder
(`yamlprims-value/1`): a binding over the Rust `yamlprims` crate, compiled to
one WebAssembly module with no imports. Zero runtime dependencies.

> **Status:** Under qualification against the `yamlprims` v1.5 specification
> and pinned `3leaps/structcorpus` conformance vectors.

```ts
import { load, loadAll, dump, loadIjson, toPlain, encode, decodeWire } from "@3leaps/yamlprims";

const doc = load("port: 8080\nenabled: true\n");
const plain = toPlain(doc.root);
const yamlOut = dump(plain);
const ijsonBytes = loadIjson(yamlOut);
const wire = encode([doc]);
const decoded = decodeWire(wire);
```

See the repository `README.md` and `docs/spec/yamlprims-spec.md` for the full
value model, resource limits, error taxonomy, and `yamlprims-value/1` wire
contract.
