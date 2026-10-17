import type { BananaApi } from "../shared/api";

declare global {
  interface Window {
    readonly banana: BananaApi;
  }
}
