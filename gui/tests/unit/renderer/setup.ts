import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// The main-process tests run under Node and need none of this.
if (typeof Element !== "undefined") {
  // jsdom has no ResizeObserver, which Radix popovers use to size themselves.
  class ResizeObserverStub {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  globalThis.ResizeObserver = ResizeObserverStub as unknown as typeof ResizeObserver;
  Element.prototype.scrollIntoView = () => {};
  Element.prototype.hasPointerCapture = () => false;
  Element.prototype.releasePointerCapture = () => {};

  afterEach(() => {
    cleanup();
    document.documentElement.removeAttribute("data-theme");
  });
}
