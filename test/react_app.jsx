// React runs the components of test/components.rs (ADR 0041), compiled to
// JSX by rust-js: on the server, then in happy-dom's DOM, clicking and typing.
// test/react.test.ts copies this beside the compiled `components.jsx`, and runs
// it with `bun test --preload ./test/happydom.ts`.

import { expect, test } from "bun:test";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";

import { App, Themed } from "./components.jsx";

globalThis.IS_REACT_ACT_ENVIRONMENT = true;
// What a badge calls on each render; the context test counts them.
globalThis.rendered = () => {};

test("renders on the server", () => {
  const html = renderToStaticMarkup(<App />);
  expect(html).toContain('<div class="card"><h2>Todos</h2><input id="');
  expect(html).toContain('<button class="add">Add</button><ul></ul><p class="empty">Nothing to do</p><span class="left">0 left</span>');
  // Effects don't run on the server.
  expect(html).toContain('<span class="ticks">0</span><button class="tick">tick</button>');
});

test("state, reducers, effects and events work in the DOM", async () => {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  await act(() => root.render(<App />));
  const $ = (selector) => container.querySelector(selector);

  // The effect ran once, after the first render.
  expect($(".ticks").textContent).toBe("10");
  await act(() => $(".tick").click());
  expect($(".ticks").textContent).toBe("11");

  // Type, as React sees typing: the value, then an `input` event.
  const type = async (text) => {
    const input = $("input");
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(input, text);
    await act(() => input.dispatchEvent(new Event("input", { bubbles: true })));
  };
  await type("milk");
  expect($("input").value).toBe("milk");
  await act(() => $(".add").click());
  await type("eggs");
  await act(() => $("input").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
  expect([...container.querySelectorAll("li")].map((li) => li.textContent)).toEqual(["milk", "eggs"]);
  expect($("input").value).toBe("");
  expect($(".empty")).toBeNull();
  expect($(".latest").textContent).toBe("eggs");
  expect($(".left").textContent).toBe("2 left");

  // A click on an item toggles it, through the reducer.
  await act(() => container.querySelectorAll("li")[0].click());
  expect(container.querySelectorAll("li")[0].className).toBe("done");
  expect($(".left").textContent).toBe("1 left");

  // Unmounting runs the effect's cleanup, which sets the state one last time.
  await act(() => root.unmount());
  expect(container.innerHTML).toBe("");
});

test("context and memo", async () => {
  const renders = [];
  globalThis.rendered = (label) => renders.push(label);
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  await act(() => root.render(<Themed />));
  const badges = () => [...container.querySelectorAll(".badge")].map((b) => `${b.className}: ${b.textContent}`);
  // Outside the provider, the context's default; inside, its value.
  expect(badges()).toEqual(["badge light: outside", "badge light: inside", "badge light: even"]);
  expect(renders).toEqual(["outside", "inside", "even"]);

  // Themed renders again, but no badge does: their props are the same, and
  // "odd!" counts as "even" for memo_with's comparison.
  renders.length = 0;
  await act(() => container.querySelector(".click").click());
  expect(container.querySelector(".click").textContent).toBe("1");
  expect(renders).toEqual([]);
  expect(badges()).toEqual(["badge light: outside", "badge light: inside", "badge light: even"]);

  // A new context value renders what reads it, memo or not; the badge
  // outside the provider doesn't read it. The loose badge keeps the props
  // its comparison accepted, "even".
  await act(() => container.querySelector(".theme").click());
  expect(renders).toEqual(["inside", "even"]);
  expect(badges()).toEqual(["badge light: outside", "badge dark: inside", "badge dark: even"]);
  await act(() => root.unmount());
});
