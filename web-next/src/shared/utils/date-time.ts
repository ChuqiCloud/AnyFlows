const DATE_TIME_FORMATTER = new Intl.DateTimeFormat("zh-CN", {
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hour12: false,
});

const DATE_FORMATTER = new Intl.DateTimeFormat("zh-CN", {
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
});

function parseDate(value?: string | number | Date | null) {
  if (!value) {
    return null;
  }

  const date = value instanceof Date ? value : new Date(value);

  return Number.isNaN(date.getTime()) ? null : date;
}

export function formatDateTime(value?: string | number | Date | null) {
  const date = parseDate(value);

  return date ? DATE_TIME_FORMATTER.format(date) : "-";
}

export function formatDate(value?: string | number | Date | null) {
  const date = parseDate(value);

  return date ? DATE_FORMATTER.format(date) : "-";
}
