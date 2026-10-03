import { createFileRoute } from "@tanstack/react-router";
import { PageStub } from "../components/PageStub";

export const Route = createFileRoute("/")({
  component: () => <PageStub title="Inicio">Hero destacado y filas del catálogo (Fase 2).</PageStub>,
});
