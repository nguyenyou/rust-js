// A DOM for the tests (ADR 0026): happy-dom's `window`, `document` and
// friends as globals, as a page would have them. Loaded with
// `bun test --preload ./test/happydom.ts`.
import { GlobalRegistrator } from "@happy-dom/global-registrator";

GlobalRegistrator.register();
