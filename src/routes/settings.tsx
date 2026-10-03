import { createFileRoute } from "@tanstack/react-router";
import { PageStub } from "../components/PageStub";

export const Route = createFileRoute("/settings")({
  component: () => (
    <PageStub title="Ajustes">
      Catálogo, subtítulos, reproducción, torrent y almacenamiento (Fase 6).
    </PageStub>
  ),
});
