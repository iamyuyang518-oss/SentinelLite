import { useId, useState, type ReactNode } from "react";

export interface ExpandableSectionProps {
  title: string;
  description?: ReactNode;
  summary?: ReactNode;
  children: ReactNode;
  defaultOpen?: boolean;
  headingLevel?: 2 | 3 | 4;
  className?: string;
  id?: string;
}

/**
 * Shared disclosure surface for secondary detail.
 *
 * The trigger is a real button and stays in place while the region below it grows or collapses.
 * The region remains mounted so the grid-row transition can animate to its true content height;
 * while closed it is inert and hidden from assistive technology.
 */
export function ExpandableSection({
  title,
  description,
  summary,
  children,
  defaultOpen = false,
  headingLevel = 2,
  className,
  id,
}: ExpandableSectionProps) {
  const generatedId = useId();
  const baseId = id ?? generatedId;
  const titleId = `${baseId}-title`;
  const descriptionId = `${baseId}-description`;
  const summaryId = `${baseId}-summary`;
  const regionId = `${baseId}-region`;
  const [open, setOpen] = useState(defaultOpen);

  const Heading = headingLevel === 2 ? "h2" : headingLevel === 3 ? "h3" : "h4";
  const describedBy = [
    description === undefined ? null : descriptionId,
    summary === undefined ? null : summaryId,
  ]
    .filter((value): value is string => value !== null)
    .join(" ");

  return (
    <section
      className={className === undefined ? "expandable" : `expandable ${className}`}
      data-open={open ? "true" : "false"}
    >
      <Heading className="expandable__heading">
        <button
          type="button"
          className="expandable__trigger"
          aria-expanded={open}
          aria-controls={regionId}
          aria-labelledby={titleId}
          aria-describedby={describedBy.length > 0 ? describedBy : undefined}
          onClick={() => setOpen((value) => !value)}
        >
          <span className="expandable__heading-text">
            <span className="expandable__title" id={titleId}>
              {title}
            </span>
            {description === undefined ? null : (
              <span className="expandable__description" id={descriptionId}>
                {description}
              </span>
            )}
          </span>
          {summary === undefined ? null : (
            <span className="expandable__summary" id={summaryId}>
              {summary}
            </span>
          )}
          <span className="expandable__chevron" aria-hidden="true" />
        </button>
      </Heading>

      <div
        className="expandable__region"
        id={regionId}
        role="region"
        aria-labelledby={titleId}
        aria-hidden={open ? undefined : true}
        inert={open ? undefined : true}
      >
        <div className="expandable__clip">
          <div className="expandable__content">{children}</div>
        </div>
      </div>
    </section>
  );
}
