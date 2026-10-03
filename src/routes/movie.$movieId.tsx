import { createFileRoute, notFound } from "@tanstack/react-router";
import { PageStub } from "../components/PageStub";

export const Route = createFileRoute("/movie/$movieId")({
  params: {
    parse: ({ movieId }) => {
      const id = Number(movieId);
      if (!Number.isInteger(id) || id <= 0) throw notFound();
      return { movieId: id };
    },
    stringify: ({ movieId }) => ({ movieId: String(movieId) }),
  },
  component: MoviePage,
});

function MoviePage() {
  const { movieId } = Route.useParams();
  return <PageStub title="Ficha">Película {movieId}: sinopsis, versiones y similares (Fase 2).</PageStub>;
}
