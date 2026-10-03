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
