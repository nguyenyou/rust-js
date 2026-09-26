// React 19.3 runs the components of test/apis.rs (ADR 0043), compiled by
// rust-js, in happy-dom's DOM. test/react.test.ts copies this beside the
// compiled `apis.jsx`, with the lazily loaded `lazy-card.jsx`, and runs it
// with `bun test --preload ./test/happydom.ts`.

import { expect, test } from "bun:test";
import { act } from "react";
import { createRoot } from "react-dom/client";

import { Misc, Places, Refs, Signup, Store, Suspended, mount, page_html, page_stream } from "./apis.jsx";

globalThis.IS_REACT_ACT_ENVIRONMENT = true;

// What the Rust reaches through its `extern` block.
const logged = [];
globalThis.log = (what) => logged.push(what);
let resolveGreeting;
globalThis.greeting = new Promise((resolve) => (resolveGreeting = resolve));
const listeners = new Set();
let storeValue = 1;
globalThis.store = {
  subscribe: (notify) => (listeners.add(notify), () => listeners.delete(notify)),
  get: () => storeValue,
};
globalThis.portalTarget = document.createElement("div");
document.body.append(globalThis.portalTarget);
globalThis.flushedText = () => document.querySelector(".flush").textContent;

async function render(element) {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  await act(() => root.render(element));
  return { container, root, $: (selector) => container.querySelector(selector) };
}

test("use() suspends until its promise resolves, under Suspense", async () => {
  const { $, root } = await render(<Suspended />);
  expect($(".loading").textContent).toBe("Loading");
  await act(async () => resolveGreeting("Hello from a promise"));
  expect($(".greeting").textContent).toBe("Hello from a promise");
  await act(() => root.unmount());
});

test("an external store, a Transition, and a deferred value", async () => {
  const { $, root } = await render(<Store />);
  expect($(".store").textContent).toBe("1");
  await act(() => {
    storeValue = 2;
    for (const notify of listeners) notify();
  });
  expect($(".store").textContent).toBe("2");
  await act(() => $(".transition").click());
  expect([$(".deferred").textContent, $(".pending").textContent]).toEqual(["7", "idle"]);
  await act(() => root.unmount());
  expect(listeners.size).toBe(0);
});

test("a form action with its state, an optimistic value and its status", async () => {
  const { $, root } = await render(<Signup />);
  expect([$(".names").textContent, $(".status").textContent, $(".action-pending").textContent]).toEqual(["", "ready", "no"]);
  // A click, as a person submits. (happy-dom's requestSubmit() gives React
  // a submitter that its FormData then rejects.)
  await act(async () => $("button").click());
  expect($(".names").textContent).toBe("ada;");
  await act(() => root.unmount());
});

test("refs, an imperative handle, a ref callback's cleanup, effects and an effect event", async () => {
  logged.length = 0;
  const { $, root } = await render(<Refs />);
  // Layout effects run before effects; refs are set by then.
  expect(logged).toEqual(["attached", "layout", "element set", "handle from FancyInput", "shown"]);
  logged.length = 0;
  await act(() => $(".hide").click());
  // The effect event sees the latest state, without being a dependency.
  expect(logged).toEqual(["detached", "hidden"]);
  await act(() => root.unmount());
});

test("Activity keeps hidden state; a portal renders elsewhere; flushSync applies at once", async () => {
  logged.length = 0;
  const { $, root } = await render(<Places />);
  await act(() => $(".count").click());
  await act(() => $(".count").click());
  await act(() => $(".toggle").click());
  expect($(".count").style.display).toBe("none");
  await act(() => $(".toggle").click());
  expect([$(".count").textContent, $(".count").style.display]).toEqual(["2", ""]);
  expect(globalThis.portalTarget.textContent).toBe("in the portal");
  await act(() => $(".flush").click());
  expect(logged).toEqual(["flushed 1"]);
  await act(() => root.unmount());
});

test("keyed fragments, styles, raw HTML, attributes, a lazy component and a Profiler", async () => {
  logged.length = 0;
  const { $, container, root } = await render(<Misc />);
  expect([...container.querySelectorAll("li")].map((li) => li.textContent)).toEqual(["1", "·", "2", "·"]);
  const styled = $(".styled").style;
  expect([styled.color, styled.fontSize, styled.getPropertyValue("--gap")]).toEqual(["red", "12px", "4px"]);
  expect($(".raw").innerHTML).toBe("<i>raw</i>");
  // useReducer's initializer: 20 * 2 + 2.
  expect($(".id").textContent).toBe("42");
  expect($(".id").getAttribute("data-id")).toMatch(/^_r_/);
  await act(async () => await new Promise((resolve) => setTimeout(resolve, 50)));
  expect($(".lazy").textContent).toBe("lazy card");
  expect(logged).toContain("misc mount");
  await act(() => root.unmount());
});

test("rendering to a string and a stream, and a root, with their options", async () => {
  expect(page_html()).toBe('<p class="page">id <!-- -->_s-R_0_</p>');
  const stream = await page_stream();
  expect(await new Response(stream).text()).toContain('<p class="page">id <!-- -->_w-R_0_</p>');
  const container = document.createElement("div");
  let root;
  await act(() => (root = mount(container)));
  expect(container.textContent).toMatch(/^id _c-r_/);
  await act(() => root.unmount());
});
