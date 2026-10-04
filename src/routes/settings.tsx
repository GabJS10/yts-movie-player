import { createFileRoute, useRouterState } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { useSettings } from "../api/queries";
import { ErrorState } from "../components/ErrorState";
import { CatalogSection } from "../components/settings/CatalogSection";
import { PlaybackSection } from "../components/settings/PlaybackSection";
import { TorrentSection } from "../components/settings/SoonSections";
import { StorageSection } from "../components/settings/StorageSection";
import { SubtitlesSection } from "../components/settings/SubtitlesSection";

export const Route = createFileRoute("/settings")({ component: SettingsPage });

const SECTIONS = [
  { id: "s-catalogo", label: "Catálogo" },
  { id: "s-subs", label: "Subtítulos" },
  { id: "s-play", label: "Reproducción" },
  { id: "s-torrent", label: "Torrent" },
  { id: "s-disk", label: "Almacenamiento" },
];

/** Highlights the section in view (scroll spy). */
function useActiveSection(ready: boolean) {
  const [active, setActive] = useState(SECTIONS[0]?.id ?? "");
  useEffect(() => {
    if (!ready) return;
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) if (e.isIntersecting) setActive(e.target.id);
      },
      { rootMargin: "-30% 0px -60% 0px" },
    );
    document.querySelectorAll("[data-set-section]").forEach((el) => io.observe(el));
    return () => io.disconnect();
  }, [ready]);
  return [active, setActive] as const;
}

function SettingsPage() {
  const settings = useSettings();
  const [active, setActive] = useActiveSection(!!settings.data);
  // Deep links such as /settings#s-subs (from the player): scroll once the sections exist.
  const hash = useRouterState({ select: (s) => s.location.hash });
  const loaded = !!settings.data;
  useEffect(() => {
    if (loaded && hash) document.getElementById(hash)?.scrollIntoView?.();
  }, [loaded, hash]);

  const jump = (id: string) => {
    const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    document.getElementById(id)?.scrollIntoView?.({ behavior: reduce ? "auto" : "smooth" });
    setActive(id);
  };

  return (
    <div className="min-h-screen px-gutter pt-[calc(var(--spacing-nav)+36px)] pb-24">
      <h1 className="m-0 mb-1.5 text-headline font-[850] uppercase stretch-condensed">Ajustes</h1>
      <p className="m-0 mb-8 text-[15px] text-muted">Los cambios se guardan al momento.</p>

      {settings.isError ? (
        <ErrorState error={settings.error} onRetry={() => void settings.refetch()} />
      ) : (
        <div className="grid grid-cols-[200px_minmax(0,760px)] gap-14 max-[900px]:grid-cols-[minmax(0,1fr)] max-[900px]:gap-0">
          <nav
            aria-label="Secciones"
            className="sticky top-[calc(var(--spacing-nav)+32px)] grid gap-0.5 self-start max-[900px]:static max-[900px]:mb-6 max-[900px]:flex max-[900px]:overflow-x-auto"
          >
            {SECTIONS.map((s) => (
              <a
                key={s.id}
                href={`#${s.id}`}
                aria-current={active === s.id ? "true" : undefined}
                onClick={(e) => {
                  e.preventDefault();
                  jump(s.id);
                }}
                className="rounded-md px-3 py-2 text-[14.5px] font-medium whitespace-nowrap text-text-2 hover:bg-white/4 hover:text-text aria-[current]:bg-white/6 aria-[current]:font-bold aria-[current]:text-text"
              >
                {s.label}
              </a>
            ))}
          </nav>
          {settings.data ? (
            <div>
              <CatalogSection urls={settings.data.apiBaseUrls} />
              <SubtitlesSection settings={settings.data} />
              <PlaybackSection settings={settings.data} />
              <TorrentSection settings={settings.data} />
              <StorageSection settings={settings.data} />
            </div>
          ) : (
            <div aria-busy="true" aria-label="Cargando ajustes">
              <div className="skeleton mb-4 h-8 w-48" />
              <div className="skeleton mb-3 h-28" />
              <div className="skeleton mb-3 h-12" />
              <div className="skeleton h-12" />
            </div>
          )}
        </div>
      )}
    </div>
  );
}
