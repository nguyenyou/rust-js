import { File } from "@bjorn3/browser_wasi_shim";
import { compile } from "./rust/compiler.js";

self.onmessage = async ({ data }) => {
  try {
    const loaded = {
      module: data.module,
      sysroot: new Map(data.sysroot.map(([name, bytes]) => [name, new File(bytes, { readonly: true })])),
      webCrate: new File(data.web, { readonly: true }),
      reactCrate: new File(data.react, { readonly: true }),
      examples: [],
    };
    self.postMessage({ result: await compile(loaded, data.sources, data.root, data.test) });
  } catch (error) { self.postMessage({ error: String(error) }); }
};
