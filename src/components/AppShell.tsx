import { Link, useRouterState } from "@tanstack/react-router";
import { useEffect, useState, type ReactNode } from "react";
import { useDownloadEvents, useDownloads, useMoveEvents } from "../api/queries";
import { activeCount as countActive } from "../lib/downloads";
import { formatSpeed } from "../lib/format";
import { useConnectivityWatch } from "../lib/useConnectivityWatch";
import { useConnectivity } from "../store/connectivity";
import { useSwarmStore } from "../store/swarm";
import { Icon, type IconName } from "./Icon";
import { ToastHost } from "./Toast";

type NavItem = {
  to: "/" | "/search" | "/my-list" | "/downloads" | "/settings";
  label: string;
  icon: IconName;
};

const PRIMARY: NavItem[] = [
  { to: "/", label: "Inicio", icon: "home" },
  { to: "/search", label: "Buscar", icon: "search" },
  { to: "/my-list", label: "Mi lista", icon: "heart" },
  { to: "/downloads", label: "Descargas", icon: "download" },
];
const COMPACT: NavItem[] = [...PRIMARY, { to: "/settings", label: "Ajustes", icon: "settings" }];

const iconBtn =
  "relative inline-grid size-10 place-items-center rounded-full text-text-2 transition-colors hover:bg-white/8 hover:text-text";

function useScrolled(threshold = 40) {
  const [scrolled, setScrolled] = useState(false);
  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > threshold);
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, [threshold]);
  return scrolled;
}

export function AppShell({ children }: { children: ReactNode }) {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const scrolled = useScrolled();
  const isPlayer = pathname.startsWith("/play/");
  // Home and the movie page open over full-bleed artwork; the bar turns solid on scroll.
  const overArtwork = pathname === "/" || pathname.startsWith("/movie/");
  const solid = scrolled || !overArtwork;

  // The counter follows download://changed; the list itself is polled only on the Downloads page.
  useDownloadEvents();
  useMoveEvents();
  useConnectivityWatch();
  const activeCount = countActive(useDownloads().data);
  // torrent://stats only covers the open stream (IPC v0.10.1); downloads alone show just the count.
  const speed = useSwarmStore((s) => s.totalBps);
  const offline = useConnectivity((s) => s.offline);

  if (isPlayer) return <main id="main">{children}</main>;

  return (
    <>
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:fixed focus:top-3 focus:left-3 focus:z-[60] focus:bg-surface focus:px-3 focus:py-2"
      >
        Saltar al contenido
      </a>
      <header
        className={`fixed inset-x-0 top-0 z-50 flex h-nav items-center gap-10 px-gutter transition-colors duration-300 max-[900px]:gap-4 ${
          solid
            ? "bg-ground shadow-[0_1px_0_var(--color-line)]"
            : "bg-linear-to-b from-black/85 to-transparent"
        }`}
      >
        <Link to="/" aria-label="YTS Player, inicio" className="flex items-baseline gap-1.5 stretch-semi">
          <b className="text-[26px] leading-none font-black text-green" style={{ fontStretch: "75%" }}>
            YTS
          </b>
          <span className="text-[15px] font-medium tracking-[0.14em] uppercase">Player</span>
        </Link>

        <nav aria-label="Principal" className="flex gap-1.5 max-[900px]:hidden">
          {PRIMARY.map((item) => (
            <Link
              key={item.to}
              to={item.to}
              activeOptions={{ exact: item.to === "/" }}
              className="relative rounded-md px-3 py-2 text-sm font-medium text-text-2 transition-colors hover:text-text data-[status=active]:font-bold data-[status=active]:text-text data-[status=active]:after:absolute data-[status=active]:after:inset-x-3 data-[status=active]:after:bottom-0 data-[status=active]:after:h-0.5 data-[status=active]:after:rounded-xs data-[status=active]:after:bg-green"
            >
              {item.label}
            </Link>
          ))}
        </nav>

        <div className="ml-auto flex items-center gap-1">
          <span role="status" className="contents">
            {offline && (
              <Link
                to="/downloads"
                className="mr-2 inline-flex h-8 items-center gap-2 rounded-full border border-line px-3 text-xs text-text-2 hover:border-line-hi hover:text-text"
                title="El catálogo no responde. Lo que descargaste se ve sin conexión desde Descargas."
              >
                <span className="size-1.5 rounded-full shadow-[inset_0_0_0_1.5px_var(--color-muted)]" />
                Sin conexión
              </Link>
            )}
          </span>
          {!offline && activeCount > 0 && (
            <span
              className="mr-2 inline-flex h-8 items-center gap-2 rounded-full border border-line px-3 text-xs text-text-2 tnum max-[900px]:hidden"
              title="Actividad del motor torrent"
            >
              <span className="size-1.5 rounded-full bg-green shadow-[0_0_0_3px_var(--color-green-wash)]" />
              <Icon name="down" size={14} />
              {speed > 0 ? `${formatSpeed(speed)} · ` : ""}
              {activeCount} {activeCount === 1 ? "activa" : "activas"}
            </span>
          )}
          <Link to="/search" aria-label="Buscar" className={iconBtn}>
            <Icon name="search" />
          </Link>
          <Link
            to="/downloads"
            aria-label={`Descargas${activeCount ? `, ${activeCount} ${activeCount === 1 ? "activa" : "activas"}` : ""}`}
            className={iconBtn}
          >
            <Icon name="download" />
            {activeCount > 0 && (
              <span className="absolute top-[3px] right-px h-4 min-w-4 rounded-full bg-green px-1 text-center text-[10px] leading-4 font-extrabold text-on-green shadow-[0_0_0_2px_var(--color-ground)] tnum">
                {activeCount}
              </span>
            )}
          </Link>
          <Link to="/settings" aria-label="Ajustes" className={iconBtn}>
            <Icon name="settings" />
          </Link>
        </div>
      </header>

      <main id="main" tabIndex={-1} className="outline-none max-[900px]:pb-16">
        {children}
      </main>
      <ToastHost />

      <nav
        aria-label="Principal (compacta)"
        className="fixed inset-x-0 bottom-0 z-[60] hidden h-16 items-center justify-around border-t border-line bg-ground/97 max-[900px]:flex"
      >
        {COMPACT.map((item) => (
          <Link
            key={item.to}
            to={item.to}
            activeOptions={{ exact: item.to === "/" }}
            className="grid justify-items-center gap-0.5 px-2.5 py-1.5 text-[11px] text-muted data-[status=active]:text-green"
          >
            <Icon name={item.icon} />
            {item.label}
          </Link>
        ))}
      </nav>
    </>
  );
}
