import { Fragment, useState, type ReactNode } from "react";

type VirtualTableProps<T> = {
  items: T[];
  rowKey: (item: T) => string;
  columnCount: number;
  headers: ReactNode;
  renderRow: (item: T) => ReactNode;
  rowHeight?: number;
  ariaLabel: string;
};

const OVERSCAN_ROWS = 12;
const VIEWPORT_HEIGHT = 620;

/**
 * Keeps the full result set addressable while mounting only the rows near the
 * scroll position. The fixed row height is deliberately explicit so a
 * 10,000-row plan does not turn into a 10,000-node React render.
 */
export function VirtualTable<T>({
  items,
  rowKey,
  columnCount,
  headers,
  renderRow,
  rowHeight = 48,
  ariaLabel,
}: VirtualTableProps<T>) {
  const [scrollTop, setScrollTop] = useState(0);
  const visibleCount = Math.ceil(VIEWPORT_HEIGHT / rowHeight);
  const firstVisible = Math.floor(scrollTop / rowHeight);
  const start = Math.max(0, firstVisible - OVERSCAN_ROWS);
  const end = Math.min(items.length, firstVisible + visibleCount + OVERSCAN_ROWS);

  return (
    <div
      className="data-table-wrap virtual-table-viewport"
      style={{ maxHeight: VIEWPORT_HEIGHT }}
      onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
    >
      <table className="data-table" aria-label={ariaLabel} aria-rowcount={items.length + 1}>
        <thead>{headers}</thead>
        <tbody>
          {start > 0 ? (
            <tr aria-hidden="true" className="virtual-spacer-row">
              <td colSpan={columnCount} style={{ height: start * rowHeight }} />
            </tr>
          ) : null}
          {items.slice(start, end).map((item) => (
            <Fragment key={rowKey(item)}>{renderRow(item)}</Fragment>
          ))}
          {end < items.length ? (
            <tr aria-hidden="true" className="virtual-spacer-row">
              <td colSpan={columnCount} style={{ height: (items.length - end) * rowHeight }} />
            </tr>
          ) : null}
        </tbody>
      </table>
    </div>
  );
}
