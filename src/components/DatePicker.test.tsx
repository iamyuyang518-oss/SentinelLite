import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { DatePicker } from "./DatePicker";

function todayValue(): string {
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(
    now.getDate(),
  ).padStart(2, "0")}`;
}

describe("DatePicker", () => {
  it("opens a small calendar and selects a specific day", () => {
    const onChange = vi.fn();
    render(<DatePicker label="结束日期" value="2026-11-02" onChange={onChange} />);

    fireEvent.click(screen.getByRole("button", { name: /结束日期/ }));

    const dialog = screen.getByRole("dialog", { name: "选择结束日期" });
    expect(within(dialog).getByText("2026年11月")).toBeInTheDocument();

    fireEvent.click(
      within(dialog).getByRole("button", { name: "2026年11月3日" }),
    );

    expect(onChange).toHaveBeenCalledWith("2026-11-03");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("selects today or clears the date", () => {
    const onChange = vi.fn();
    const { rerender } = render(
      <DatePicker label="开始日期" value="" onChange={onChange} />,
    );

    fireEvent.click(screen.getByRole("button", { name: /开始日期/ }));
    fireEvent.click(
      within(screen.getByRole("dialog", { name: "选择开始日期" })).getByRole(
        "button",
        { name: "今天" },
      ),
    );
    expect(onChange).toHaveBeenLastCalledWith(todayValue());

    rerender(
      <DatePicker label="开始日期" value="2026-11-02" onChange={onChange} />,
    );
    fireEvent.click(screen.getByRole("button", { name: /开始日期/ }));
    fireEvent.click(
      within(screen.getByRole("dialog", { name: "选择开始日期" })).getByRole(
        "button",
        { name: "清除" },
      ),
    );
    expect(onChange).toHaveBeenLastCalledWith("");
  });

  it("closes on Escape and returns focus to the trigger", () => {
    render(<DatePicker label="开始日期" value="" onChange={() => undefined} />);

    const trigger = screen.getByRole("button", { name: /开始日期/ });
    fireEvent.click(trigger);
    expect(screen.getByRole("dialog", { name: "选择开始日期" })).toBeInTheDocument();

    fireEvent.keyDown(document, { key: "Escape" });

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });
});
