import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { App } from "@/App";

afterEach(cleanup);

test("the shell renders its landmarks and an empty state", () => {
  render(<App />);
  expect(screen.getByRole("navigation", { name: "Sections" })).toBeTruthy();
  expect(screen.getByRole("complementary", { name: "Sessions" })).toBeTruthy();
  expect(screen.getByRole("main").textContent).toContain("Open a workspace");
});
