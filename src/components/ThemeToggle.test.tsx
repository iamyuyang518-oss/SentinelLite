import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ThemeToggle } from "./ThemeToggle";

describe("ThemeToggle", () => {
  it("offers a small button to switch from light to dark", () => {
    const onToggle = vi.fn();
    render(<ThemeToggle theme="light" onToggle={onToggle} />);

    fireEvent.click(screen.getByRole("button", { name: "切换到深色模式" }));

    expect(onToggle).toHaveBeenCalledTimes(1);
  });

  it("offers the inverse action in dark mode", () => {
    render(<ThemeToggle theme="dark" onToggle={() => undefined} />);

    expect(screen.getByRole("button", { name: "切换到浅色模式" })).toBeInTheDocument();
  });
});
