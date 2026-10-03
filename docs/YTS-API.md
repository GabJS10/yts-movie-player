# Referencia de la API de YTS (v2)

API JSON pública, solo GET y sin autenticación. Es la fuente del catálogo de la app.

## URLs base (en orden de preferencia)
1. `https://movies-api.accel.li/api/v2/`: nueva URL oficial (lo anuncia la propia API en `status_message`).
2. `https://yts.gg/api/v2/`: respaldo.

YTS cambia de dominio a menudo. La app **nunca** debe tener la URL fija en el código: lista configurable, failover automático y override desde Ajustes.

## Endpoints

| Endpoint | Uso en la app |
|---|---|
| `list_movies.json` | Filas de la Home, búsqueda y filtros |
| `movie_details.json?movie_id=X&with_images=true&with_cast=true` | Ficha de la película |
| `movie_suggestions.json?movie_id=X` | Fila "Similares" |

### Parámetros de `list_movies.json`

| Parámetro | Valores |
|---|---|
| `limit` | 1–50 (por defecto 20) |
| `page` | 1, 2, 3… |
| `quality` | `480p`, `720p`, `1080p`, `1080p.x265`, `2160p`, `3D`, `all` |
| `minimum_rating` | 0–9 |
| `query_term` | título, actor, director o código IMDb (`tt0133093`) |
| `genre` | `action`, `comedy`, `drama`, `horror`, `sci-fi`, `animation`… |
| `sort_by` | `title`, `year`, `rating`, `peers`, `seeds`, `download_count`, `like_count`, `date_added` |
| `order_by` | `desc` (por defecto) o `asc` |

## Ejemplos (se pueden importar en Postman: Import → Raw text)

```bash
# Listado general
curl --location 'https://movies-api.accel.li/api/v2/list_movies.json?limit=20&page=1'

# Búsqueda
curl --location 'https://movies-api.accel.li/api/v2/list_movies.json?query_term=matrix&limit=20'

# Filtros: Acción 1080p, rating >= 7
curl --location 'https://movies-api.accel.li/api/v2/list_movies.json?genre=action&quality=1080p&minimum_rating=7&sort_by=rating&order_by=desc&limit=50'

# Tendencias / Recientes
curl --location 'https://movies-api.accel.li/api/v2/list_movies.json?sort_by=download_count&limit=20'
curl --location 'https://movies-api.accel.li/api/v2/list_movies.json?sort_by=date_added&limit=20'

# Ficha (The Matrix Resurrections)
curl --location 'https://movies-api.accel.li/api/v2/movie_details.json?movie_id=38698&with_images=true&with_cast=true'

# Similares
curl --location 'https://movies-api.accel.li/api/v2/movie_suggestions.json?movie_id=38698'

# Dominio de respaldo
curl --location 'https://yts.gg/api/v2/list_movies.json?query_term=matrix&limit=5'
```

## Campos relevantes de una película

| Campo | Uso |
|---|---|
| `id` | ID interno de YTS (detalles, sugerencias, clave en la DB) |
| `imdb_code` | Búsqueda de subtítulos en OpenSubtitles |
| `title_long`, `year`, `rating`, `runtime`, `genres`, `summary` | Tarjetas y ficha |
| `yt_trailer_code` | ID del tráiler de YouTube |
| `small/medium/large_cover_image`, `background_image` | Portadas y banner (se cachean en disco) |
| `torrents[]` | Una entrada por versión (calidad/tipo) |

### `torrents[]`

| Campo | Notas |
|---|---|
| `hash` | Infohash con el que se arma el magnet: `magnet:?xt=urn:btih:<hash>&dn=<título>&tr=<trackers>` |
| `quality` | `720p`, `1080p`, `2160p`… |
| `type` | `bluray` / `web` |
| `video_codec` | `x264` se reproduce en WebKitGTK; `x265` (normalmente 2160p, 10-bit) probablemente no, así que se usa "Abrir en VLC" |
| `seeds` / `peers` | Se muestran en el selector de calidad |
| `size_bytes` | Usar este campo para los cálculos (`size` es texto, por ejemplo "2.73 GB") |

## Peculiaridades que hay que manejar
- `is_repack` llega como `"0"` o `""` (string inconsistente), así que el modelo serde debe ser tolerante.
- Si una búsqueda no encuentra nada, `data` **no trae** la clave `movies` (usar `#[serde(default)]`).
- Los campos `url` e imágenes traen el dominio dentro (`https://yts.gg/...`).
- Comprobar siempre `status == "ok"`.
