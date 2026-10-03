import type { AppError, ErrorCode } from "./types";

// User-facing Spanish copy for every contract error code: what happened and what to do next.
export const ERROR_COPY: Record<ErrorCode, { title: string; action: string }> = {
  network: { title: "Sin conexión", action: "Revisa tu conexión a internet y vuelve a intentarlo." },
  api_unavailable: {
    title: "El catálogo de YTS no responde",
    action: "Prueba más tarde o añade otro servidor en Ajustes › Catálogo.",
  },
  not_found: { title: "No encontramos esta película", action: "Vuelve al inicio y busca otra." },
  invalid_input: { title: "Algún dato no es válido", action: "Revisa los valores e inténtalo de nuevo." },
  torrent: { title: "El motor torrent falló", action: "Vuelve a intentarlo o elige otra versión." },
  no_peers: {
    title: "Nadie está compartiendo esta versión",
    action: "Prueba otra calidad con más seeds.",
  },
  subtitles_auth: {
    title: "Falta la clave de OpenSubtitles",
    action: "Añádela en Ajustes › Subtítulos.",
  },
  subtitles_quota: {
    title: "Se agotó la cuota diaria de subtítulos",
    action: "Carga un archivo .srt o vuelve a intentarlo mañana.",
  },
  external_player_missing: {
    title: "No encontramos VLC",
    action: "Instálalo o indica otro reproductor en Ajustes › Reproducción.",
  },
  io: { title: "No se pudo escribir en el disco", action: "Libera espacio o revisa la carpeta de datos." },
  db: {
    title: "No se pudieron guardar tus datos",
    action: "Reinicia la app; si sigue, revisa la carpeta de datos.",
  },
  internal: { title: "Algo salió mal", action: "Vuelve a intentarlo." },
};

export const describeError = (error: AppError) => ERROR_COPY[error.code];
