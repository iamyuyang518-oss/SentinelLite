import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { formatDateLabel, formatDateValue, parseDateValue } from "../lib/dateRange";

const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"];
const POPOVER_WIDTH = 272;
const POPOVER_HEIGHT = 330;

function CalendarIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      <rect x="3" y="4.5" width="14" height="12.5" rx="2" />
      <path d="M6.5 3v3M13.5 3v3M3 8h14" />
    </svg>
  );
}

export function DatePicker({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  const [anchor, setAnchor] = useState<{ top: number; left: number } | null>(null);
  const [viewDate, setViewDate] = useState(
    () => parseDateValue(value) ?? new Date(),
  );
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const popoverId = useId();
  const open = anchor !== null;

  function openCalendar(): void {
    const trigger = triggerRef.current;
    if (trigger === null) {
      return;
    }
    const rect = trigger.getBoundingClientRect();
    const left = Math.max(
      8,
      Math.min(rect.left, window.innerWidth - POPOVER_WIDTH - 8),
    );
    const below = rect.bottom + 8;
    const top =
      below + POPOVER_HEIGHT <= window.innerHeight
        ? below
        : Math.max(8, rect.top - POPOVER_HEIGHT - 8);
    setViewDate(parseDateValue(value) ?? new Date());
    setAnchor({ top, left });
  }

  function close(returnFocus: boolean): void {
    setAnchor(null);
    if (returnFocus) {
      triggerRef.current?.focus();
    }
  }

  useEffect(() => {
    if (!open) {
      return;
    }

    function handlePointerDown(event: PointerEvent): void {
      const target = event.target as Node;
      if (
        triggerRef.current?.contains(target) ||
        popoverRef.current?.contains(target)
      ) {
        return;
      }
      setAnchor(null);
    }

    function handleKeyDown(event: KeyboardEvent): void {
      if (event.key !== "Escape") {
        return;
      }
      setAnchor(null);
      triggerRef.current?.focus();
    }

    function handleViewportChange(): void {
      setAnchor(null);
    }

    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    window.addEventListener("resize", handleViewportChange);
    window.addEventListener("scroll", handleViewportChange, true);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("resize", handleViewportChange);
      window.removeEventListener("scroll", handleViewportChange, true);
    };
  }, [open]);

  useLayoutEffect(() => {
    if (!open) {
      return;
    }
    const selected = popoverRef.current?.querySelector<HTMLButtonElement>(
      '[data-selected="true"]',
    );
    const firstDay = popoverRef.current?.querySelector<HTMLButtonElement>(
      ".date-picker__day:not(.is-empty)",
    );
    (selected ?? firstDay)?.focus();
  }, [open, viewDate]);

  const year = viewDate.getFullYear();
  const month = viewDate.getMonth();
  const firstOfMonth = new Date(year, month, 1);
  const startOffset = (firstOfMonth.getDay() + 6) % 7;
  const daysInMonth = new Date(year, month + 1, 0).getDate();
  const today = new Date();
  const todayValue = formatDateValue(
    today.getFullYear(),
    today.getMonth(),
    today.getDate(),
  );
  const cells = Array.from({ length: 42 }, (_, index) => {
    const day = index - startOffset + 1;
    return day >= 1 && day <= daysInMonth ? new Date(year, month, day) : null;
  });

  return (
    <div className="date-picker">
      <button
        ref={triggerRef}
        type="button"
        className="date-picker__trigger"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={popoverId}
        aria-label={`${label}：${value.length > 0 ? formatDateLabel(value) : "未选择"}`}
        onClick={() => {
          if (open) {
            close(true);
          } else {
            openCalendar();
          }
        }}
      >
        <CalendarIcon />
        <span
          className={
            value.length > 0
              ? "date-picker__value"
              : "date-picker__value is-placeholder"
          }
        >
          {value.length > 0 ? formatDateLabel(value) : "选择日期"}
        </span>
      </button>

      {open && anchor !== null
        ? createPortal(
            <div
              ref={popoverRef}
              id={popoverId}
              className="date-picker-popover"
              style={{ top: anchor.top, left: anchor.left }}
              role="dialog"
              aria-label={`选择${label}`}
            >
              <div className="date-picker__header">
                <button
                  type="button"
                  className="date-picker__nav"
                  aria-label="上个月"
                  onClick={() => setViewDate(new Date(year, month - 1, 1))}
                >
                  ‹
                </button>
                <span className="date-picker__month">
                  {year}年{month + 1}月
                </span>
                <button
                  type="button"
                  className="date-picker__nav"
                  aria-label="下个月"
                  onClick={() => setViewDate(new Date(year, month + 1, 1))}
                >
                  ›
                </button>
              </div>

              <div className="date-picker__weekdays" aria-hidden="true">
                {WEEKDAYS.map((weekday) => (
                  <span key={weekday}>{weekday}</span>
                ))}
              </div>

              <div className="date-picker__grid">
                {cells.map((date, index) => {
                  if (date === null) {
                    return (
                      <span
                        key={`empty-${index}`}
                        className="date-picker__day is-empty"
                        aria-hidden="true"
                      />
                    );
                  }
                  const dateValue = formatDateValue(
                    date.getFullYear(),
                    date.getMonth(),
                    date.getDate(),
                  );
                  const selected = dateValue === value;
                  return (
                    <button
                      key={dateValue}
                      type="button"
                      className={[
                        "date-picker__day",
                        selected ? "is-selected" : "",
                        dateValue === todayValue ? "is-today" : "",
                      ]
                        .filter(Boolean)
                        .join(" ")}
                      aria-label={`${date.getFullYear()}年${date.getMonth() + 1}月${date.getDate()}日`}
                      aria-pressed={selected}
                      data-selected={selected ? "true" : undefined}
                      onClick={() => {
                        onChange(dateValue);
                        close(true);
                      }}
                    >
                      {date.getDate()}
                    </button>
                  );
                })}
              </div>

              <div className="date-picker__footer">
                <button
                  type="button"
                  onClick={() => {
                    const now = new Date();
                    onChange(
                      formatDateValue(
                        now.getFullYear(),
                        now.getMonth(),
                        now.getDate(),
                      ),
                    );
                    close(true);
                  }}
                >
                  今天
                </button>
                <button
                  type="button"
                  onClick={() => {
                    onChange("");
                    close(true);
                  }}
                >
                  清除
                </button>
              </div>
            </div>,
            document.body,
          )
        : null}
    </div>
  );
}
