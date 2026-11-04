export const LIST_PAGE_SIZE = 50;

interface ListPaginationProps {
  page: number;
  pageCount: number;
  onPageChange: (page: number) => void;
  label?: string;
}

export function ListPagination({
  page,
  pageCount,
  onPageChange,
  label = "列表分页",
}: ListPaginationProps) {
  if (pageCount <= 1) {
    return null;
  }

  return (
    <nav className="list-pagination" aria-label={label}>
      <span aria-live="polite">
        第 {page} / {pageCount} 页
      </span>
      <button
        type="button"
        className="button button--compact"
        disabled={page <= 1}
        onClick={() => onPageChange(page - 1)}
      >
        上一页
      </button>
      <button
        type="button"
        className="button button--compact"
        disabled={page >= pageCount}
        onClick={() => onPageChange(page + 1)}
      >
        下一页
      </button>
    </nav>
  );
}

export function paginateItems<T>(
  items: readonly T[],
  requestedPage: number,
): { items: readonly T[]; page: number; pageCount: number } {
  const pageCount = Math.max(1, Math.ceil(items.length / LIST_PAGE_SIZE));
  const page = Math.min(Math.max(1, requestedPage), pageCount);
  const start = (page - 1) * LIST_PAGE_SIZE;
  return {
    items: items.slice(start, start + LIST_PAGE_SIZE),
    page,
    pageCount,
  };
}
