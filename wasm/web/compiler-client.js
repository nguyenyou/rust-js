// One worker per compilation: termination also releases rustc's global state.
let active;

export function cancelCompile() {
  active?.("Compilation cancelled");
}

export function compileInWorker(loaded, sources, root, test) {
  cancelCompile();
  return new Promise(resolve => {
    let worker;
    let timer;
    const finish = result => {
      clearTimeout(timer);
      worker?.terminate();
      if (active === fail) active = undefined;
      resolve(result);
    };
    const fail = message => finish({
      ok: false, exit: "worker failure", files: new Map(), stderr: message,
      instantiate: 0, run: 0, memory: 0,
    });
    active = fail;
    timer = setTimeout(() => fail("Compilation timed out after 60 seconds"), 60_000);
    try {
      worker = new Worker(new URL("./compiler-worker.js", import.meta.url), { type: "module" });
      worker.onerror = event => { event.preventDefault(); fail(event.message || "Compiler worker failed"); };
      worker.onmessageerror = () => fail("Cannot decode compiler worker response");
      worker.onmessage = ({ data }) => {
        if (data?.error) fail(data.error);
        else if (data?.result && typeof data.result.ok === "boolean" && data.result.files instanceof Map) finish(data.result);
        else fail("Invalid compiler worker response");
      };
      worker.postMessage({
        module: loaded.module,
        sysroot: [...loaded.sysroot].map(([name, file]) => [name, file.data]),
        web: loaded.webCrate.data, react: loaded.reactCrate.data,
        sources, root, test,
      });
    } catch (error) { fail(String(error)); }
  });
}

addEventListener("pagehide", cancelCompile);
