import { createFileRoute } from "@tanstack/react-router";
import { PageStub } from "../components/PageStub";

export const Route = createFileRoute("/downloads")({
  component: () => <PageStub title="Descargas">Descargas en curso y biblioteca (Fase 6).</PageStub>,
});
