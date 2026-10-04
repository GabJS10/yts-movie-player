// Spanish number formatting used across the UI.
const nf1 = new Intl.NumberFormat("es-ES", { minimumFractionDigits: 1, maximumFractionDigits: 1 });
const nf0 = new Intl.NumberFormat("es-ES");

const MB = 1024 * 1024;
const GB = 1024 * MB;

export function formatBytes(bytes: number): string {
  if (bytes >= GB) return `${nf1.format(bytes / GB)} GB`;
  return `${nf0.format(Math.round(bytes / MB))} MB`;
}

/** Speeds come from the backend in bytes/s; the UI shows MB/s. */
export function formatSpeed(bps: number): string {
  return `${nf1.format(bps / MB)} MB/s`;
}

export function formatRuntime(minutes: number): string {
  if (!minutes) return "";
  return minutes >= 60 ? `${Math.floor(minutes / 60)} h ${minutes % 60} min` : `${minutes} min`;
}

/** "Quedan 1 h 2 min" / "Quedan 4 min" / "Queda menos de un minuto". */
export function timeLeft(positionS: number, durationS: number): string {
  const min = Math.round(Math.max(0, durationS - positionS) / 60);
  return min < 1 ? "Queda menos de un minuto" : `Quedan ${formatRuntime(min)}`;
}

const nfRating = new Intl.NumberFormat("es-ES", { minimumFractionDigits: 1, maximumFractionDigits: 1 });
export const formatRating = (r: number) => nfRating.format(r);

/** Seeds/peers as YTS reports them: the API caps at 100. */
export const formatCount = (n: number) => (n >= 100 ? "100+" : String(n));
