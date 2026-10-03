import { createFileRoute } from "@tanstack/react-router";
import { PageStub } from "../components/PageStub";
import { validateCatalogSearch } from "../lib/searchParams";

export const Route = createFileRoute("/search")({
  validateSearch: validateCatalogSearch,
  component: SearchPage,
});

function SearchPage() {
  const { query } = Route.useSearch();
  return (
    <PageStub title="Buscar">
      {query
        ? `Resultados para «${query}» (Fase 2).`
        : "Búsqueda con filtros de género, calidad, valoración y orden (Fase 2)."}
    </PageStub>
  );
}
