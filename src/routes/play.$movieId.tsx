import { createFileRoute, Link, notFound } from "@tanstack/react-router";
import { Icon } from "../components/Icon";

export const Route = createFileRoute("/play/$movieId")({
  params: {
    parse: ({ movieId }) => {
      const id = Number(movieId);
      if (!Number.isInteger(id) || id <= 0) throw notFound();
      return { movieId: id };
    },
    stringify: ({ movieId }) => ({ movieId: String(movieId) }),
  },
  component: PlayerPage,
});

// Full-screen surface: the shell hides its navigation on /play.
function PlayerPage() {
  const { movieId } = Route.useParams();
  return (
    <div className="fixed inset-0 z-[100] grid place-items-center bg-black p-6">
      <div className="w-full max-w-[640px]">
        <h1 className="m-0 text-headline font-[850] uppercase stretch-condensed">Reproductor</h1>
        <p className="mt-2 mb-8 text-muted">Streaming de la película {movieId} (Fase 3).</p>
        <Link
          to="/movie/$movieId"
          params={{ movieId }}
          className="inline-flex h-9 items-center gap-2 rounded-md px-3.5 text-sm font-bold shadow-[inset_0_0_0_1px_var(--color-line-hi)] hover:bg-white/4"
        >
          <Icon name="back" size={18} />
          Volver
        </Link>
      </div>
    </div>
  );
}
