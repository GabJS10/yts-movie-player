import type { QueryClient } from "@tanstack/react-query";
import { createRootRouteWithContext, Outlet } from "@tanstack/react-router";
import { AppShell } from "../components/AppShell";
import { PageStub } from "../components/PageStub";

export type RouterContext = { queryClient: QueryClient };

export const Route = createRootRouteWithContext<RouterContext>()({
  component: () => (
    <AppShell>
      <Outlet />
    </AppShell>
  ),
  notFoundComponent: () => (
    <PageStub title="No encontrado">
      Esta pantalla no existe. Vuelve al inicio desde la barra superior.
    </PageStub>
  ),
});
