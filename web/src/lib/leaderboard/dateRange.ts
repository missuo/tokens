const DATE_REGEX = /^\d{4}-\d{2}-\d{2}$/;

export interface CustomDateRange {
  from: string;
  to: string;
}

export function isValidDateString(value: string | null | undefined): value is string {
  if (!value || !DATE_REGEX.test(value)) {
    return false;
  }

  const [year, month, day] = value.split("-").map(Number);
  const parsedDate = new Date(year, month - 1, day);

  return (
    parsedDate.getFullYear() === year &&
    parsedDate.getMonth() === month - 1 &&
    parsedDate.getDate() === day
  );
}

/**
 * The viewer's own calendar date as YYYY-MM-DD.
 *
 * `toISOString().slice(0, 10)` would give the UTC date, which is the whole
 * problem this exists to avoid: daily rows are bucketed by the submitter's
 * local date, so anywhere east or west of UTC spends part of every day
 * disagreeing with it.
 */
export function toLocalDateString(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function parseCustomDateRange(
  from: string | null | undefined,
  to: string | null | undefined
): CustomDateRange | null {
  if (!isValidDateString(from) || !isValidDateString(to)) {
    return null;
  }

  // Lexicographic comparison is correct here because isValidDateString above
  // enforces the YYYY-MM-DD format, making string order identical to date order.
  if (from > to) {
    return null;
  }

  return { from, to };
}
