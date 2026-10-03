import { createFileRoute } from "@tanstack/react-router";
import { PageStub } from "../components/PageStub";

export const Route = createFileRoute("/my-list")({
  component: () => <PageStub title="Mi lista">Películas guardadas para después (Fase 5).</PageStub>,
});
