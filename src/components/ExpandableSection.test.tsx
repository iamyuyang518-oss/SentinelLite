import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ExpandableSection } from "./ExpandableSection";

function renderSection(defaultOpen = false) {
  return render(
    <ExpandableSection
      title="运行详情"
      description="采集计数、通道与健康信息。"
      summary="进程监测"
      defaultOpen={defaultOpen}
    >
      <p>展开后的内容</p>
      <button type="button">内部按钮</button>
    </ExpandableSection>,
  );
}

describe("ExpandableSection", () => {
  it("uses a native disclosure button and keeps the title fixed while collapsed", () => {
    renderSection();

    const trigger = screen.getByRole("button", { name: "运行详情" });
    const regionId = trigger.getAttribute("aria-controls");
    const region = document.getElementById(regionId ?? "");

    expect(trigger.tagName).toBe("BUTTON");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(regionId).not.toBeNull();
    expect(region).not.toBeNull();
    expect(region).toHaveAttribute("role", "region");
    expect(region).toHaveAttribute("aria-hidden", "true");
    expect(region).toHaveAttribute("inert");
    expect(within(trigger).getByText("进程监测")).toBeInTheDocument();
    expect(region?.textContent).toContain("展开后的内容");

    trigger.focus();
    expect(trigger).toHaveFocus();
  });

  it("opens and closes with the same trigger, keeping aria state in sync", () => {
    renderSection();

    const trigger = screen.getByRole("button", { name: "运行详情" });
    const region = document.getElementById(trigger.getAttribute("aria-controls") ?? "");

    fireEvent.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(region).not.toHaveAttribute("aria-hidden");
    expect(region).not.toHaveAttribute("inert");

    fireEvent.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(region).toHaveAttribute("aria-hidden", "true");
    expect(region).toHaveAttribute("inert");
  });

  it("supports a controlled initial open state", () => {
    renderSection(true);

    const trigger = screen.getByRole("button", { name: "运行详情" });
    const region = document.getElementById(trigger.getAttribute("aria-controls") ?? "");

    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(region).not.toHaveAttribute("inert");
  });
});
