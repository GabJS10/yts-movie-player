import type { ReactNode } from "react";
import type { OrderBy, SortBy } from "../api/types";
import { useT } from "../i18n";
import { genres } from "../lib/genres";
import type { CatalogSearch } from "../lib/searchParams";

type Props = { value: CatalogSearch; onChange: (patch: Partial<CatalogSearch>) => void };

const QUALITY_OPTIONS = [
  ["", null],
  ["720p", "720p"],
  ["1080p", "1080p"],
  ["2160p", "4K"],
  ["3D", "3D"],
] as const;
const RATING_OPTIONS = [0, 6, 7, 8] as const;
const SORT_OPTIONS: readonly { value: SortBy; order: OrderBy }[] = [
  { value: "download_count", order: "desc" },
  { value: "date_added", order: "desc" },
  { value: "rating", order: "desc" },
  { value: "seeds", order: "desc" },
  { value: "year", order: "desc" },
  { value: "title", order: "asc" },
];

const Field = ({ label, children, id }: { label: string; children: ReactNode; id?: string }) => (
  <div className="grid gap-1.5">
    {id ? (
      <label htmlFor={id} className="field-label">
        {label}
      </label>
    ) : (
      <span className="field-label">{label}</span>
    )}
    {children}
  </div>
);

export function SearchFilters({ value, onChange }: Props) {
  const t = useT().filters;
  return (
    <div className="mb-6 flex flex-wrap items-end gap-x-7 gap-y-4 border-b border-line pt-1.5 pb-5">
      <Field label={t.genre} id="f-genre">
        <select
          id="f-genre"
          className="select"
          data-testid="filter-genre"
          value={value.genre ?? ""}
          onChange={(e) => onChange({ genre: e.target.value || undefined })}
        >
          <option value="">{t.allGenres}</option>
          {genres().map((g) => (
            <option key={g.value} value={g.value}>
              {g.label}
            </option>
          ))}
        </select>
      </Field>
      <Field label={t.quality}>
        <div className="seg" role="group" aria-label={t.quality} data-testid="filter-quality">
          {QUALITY_OPTIONS.map(([v, label]) => (
            <button
              key={v}
              type="button"
              aria-pressed={(value.quality ?? "") === v}
              onClick={() => onChange({ quality: v || undefined })}
            >
              {label ?? t.all}
            </button>
          ))}
        </div>
      </Field>
      <Field label={t.minRating}>
        <div className="seg" role="group" aria-label={t.minRating} data-testid="filter-rating">
          {RATING_OPTIONS.map((r) => (
            <button
              key={r}
              type="button"
              aria-pressed={(value.minimumRating ?? 0) === r}
              onClick={() => onChange({ minimumRating: r || undefined })}
            >
              {r ? `${r}+` : t.all}
            </button>
          ))}
        </div>
      </Field>
      <Field label={t.sortBy} id="f-sort">
        <select
          id="f-sort"
          className="select"
          data-testid="filter-sort"
          value={value.sortBy ?? "download_count"}
          onChange={(e) => {
            const opt = SORT_OPTIONS.find((o) => o.value === e.target.value);
            onChange({ sortBy: opt?.value, orderBy: opt?.order === "asc" ? "asc" : undefined });
          }}
        >
          {SORT_OPTIONS.map((o) => (
            <option key={o.value} value={o.value}>
              {t.sorts[o.value]}
            </option>
          ))}
        </select>
      </Field>
    </div>
  );
}
