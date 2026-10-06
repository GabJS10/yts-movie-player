import { useState, type FormEvent } from "react";
import { useApiStatus, useUpdateSettings } from "../../api/queries";
import type { ApiEndpointStatus } from "../../api/types";
import { useT } from "../../i18n";
import { validateBaseUrl } from "../../lib/settings";
import { showToast } from "../../store/toast";
import { Icon } from "../Icon";
import { SetSection } from "./controls";

const host = (baseUrl: string) => {
  try {
    return new URL(baseUrl).host;
  } catch {
    return baseUrl;
  }
};

function EndpointStatus({
  status,
  measuring,
}: {
  status: ApiEndpointStatus | undefined;
  measuring: boolean;
}) {
  const t = useT().catalog;
  const ms = status?.latencyMs != null ? ` · ${status.latencyMs} ms` : "";
  const [dot, text] = !status
    ? ["status-standby", measuring ? t.measuring : t.notMeasured]
    : !status.ok
      ? ["status-down", t.down]
      : status.role === "active"
        ? ["status-live", `${t.inUse}${ms}`]
        : ["status-standby", `${t.backup}${ms}`];
  return (
    <span className="inline-flex items-center gap-[7px] text-[12.5px] whitespace-nowrap text-text-2 tnum">
      <i className={`status-dot ${dot}`} aria-hidden="true" />
      {text}
    </span>
  );
}

const iconBtn =
  "inline-grid size-9 place-items-center rounded-md text-text-2 transition-colors hover:bg-white/8 hover:text-text disabled:cursor-not-allowed disabled:opacity-35 disabled:hover:bg-transparent";

/** Ajustes › Catálogo: YTS base URLs in failover order, with their live latency. */
export function CatalogSection({ urls }: { urls: string[] }) {
  const update = useUpdateSettings();
  const t = useT().catalog;
  const status = useApiStatus({ live: true });
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);

  const save = (apiBaseUrls: string[]) => update.mutate({ apiBaseUrls });
  const statusOf = (url: string) => status.data?.find((s) => s.baseUrl === url);

  const add = (e: FormEvent) => {
    e.preventDefault();
    const problem = validateBaseUrl(draft, urls);
    setError(problem);
    if (problem) return;
    save([...urls, draft.trim()]);
    setDraft("");
  };

  const test = async () => {
    const r = await status.refetch();
    if (!r.data) {
      showToast(t.measureFailed, "error");
      return;
    }
    showToast(
      r.data
        .map((s) =>
          s.ok && s.latencyMs !== null
            ? t.respondsIn(host(s.baseUrl), s.latencyMs)
            : t.notResponding(host(s.baseUrl)),
        )
        .join(" · "),
      r.data.some((s) => s.ok) ? "ok" : "error",
    );
  };

  return (
    <SetSection id="s-catalogo" title={t.title} lede={t.lede}>
      <ul className="m-0 list-none rounded-lg border border-line p-0" aria-label={t.servers}>
        {urls.map((url, i) => (
          <li
            key={url}
            className="grid grid-cols-[26px_minmax(0,1fr)_auto_auto] items-center gap-3.5 px-3.5 py-3 [&+li]:border-t [&+li]:border-line max-[520px]:grid-cols-[26px_minmax(0,1fr)_auto]"
          >
            <span className="text-[13px] font-extrabold text-muted tnum">{i + 1}</span>
            <code
              className="overflow-hidden font-mono text-[13px] text-ellipsis whitespace-nowrap"
              title={url}
            >
              {url}
            </code>
            <span className="max-[520px]:col-start-2 max-[520px]:row-start-2">
              <EndpointStatus status={statusOf(url)} measuring={status.isFetching} />
            </span>
            <span className="flex gap-0.5">
              <button
                type="button"
                className={iconBtn}
                aria-label={t.moveUp(host(url))}
                disabled={i === 0}
                onClick={() => {
                  const next = [...urls];
                  next.splice(i - 1, 0, ...next.splice(i, 1));
                  save(next);
                }}
              >
                <Icon name="arrow-up" size={18} />
              </button>
              <button
                type="button"
                className={iconBtn}
                aria-label={t.remove(host(url))}
                title={urls.length === 1 ? t.lastOne : undefined}
                disabled={urls.length === 1}
                onClick={() => save(urls.filter((u) => u !== url))}
              >
                <Icon name="x" size={18} />
              </button>
            </span>
          </li>
        ))}
      </ul>

      <form className="set-row set-row-stack" onSubmit={add} noValidate>
        <div className="flex gap-2 max-[640px]:flex-wrap">
          <input
            className="input input-mono min-w-0 flex-1 max-[640px]:basis-full"
            placeholder={t.placeholder}
            aria-label={t.addServer}
            aria-invalid={error ? true : undefined}
            aria-describedby={error ? "catalog-url-error" : undefined}
            value={draft}
            onChange={(e) => {
              setDraft(e.target.value);
              if (error) setError(null);
            }}
            inputMode="url"
            spellCheck={false}
            autoComplete="off"
          />
          <button type="submit" className="btn btn-line btn-sm">
            <Icon name="plus" size={18} />
            {t.add}
          </button>
          <button
            type="button"
            className="btn btn-line btn-sm"
            onClick={() => void test()}
            disabled={status.isFetching}
          >
            <Icon name="refresh" size={18} />
            {t.test}
          </button>
        </div>
        {error && (
          <p id="catalog-url-error" role="alert" className="!mt-0 text-danger">
            {error}
          </p>
        )}
      </form>
    </SetSection>
  );
}
