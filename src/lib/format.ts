import { getT, numberFormat, oneDecimal } from "../i18n";

// Number and duration formatting used across the UI, in the current language.

const MB = 1024 * 1024;
const GB = 1024 * MB;

export function formatBytes(bytes: number): string {
  if (bytes >= GB) return `${oneDecimal().format(bytes / GB)} GB`;
  return `${numberFormat().format(Math.round(bytes / MB))} MB`;
}

/** Speeds come from the backend in bytes/s; the UI shows MB/s. */
export function formatSpeed(bps: number): string {
  return `${oneDecimal().format(bps / MB)} MB/s`;
}

export function formatRuntime(minutes: number): string {
  if (!minutes) return "";
  const t = getT().format;
  return minutes >= 60 ? t.hoursMinutes(Math.floor(minutes / 60), minutes % 60) : t.minutes(minutes);
}

/** "Quedan 1 h 2 min" / "Quedan 4 min" / "Queda menos de un minuto". */
export function timeLeft(positionS: number, durationS: number): string {
  const min = Math.round(Math.max(0, durationS - positionS) / 60);
  const t = getT().format;
  return min < 1 ? t.lessThanAMinuteLeft : t.timeLeft(formatRuntime(min));
}

export const formatRating = (r: number) => oneDecimal().format(r);

/** Seeds/peers as YTS reports them: the API caps at 100. */
export const formatCount = (n: number) => (n >= 100 ? "100+" : String(n));
