import type { QueryClient } from "@tanstack/react-query";
import { createRootRouteWithContext, Outlet } from "@tanstack/react-router";
import { AppShell } from "../components/AppShell";
import { ErrorState } from "../components/ErrorState";
import { PageStub } from "../components/PageStub";
import { toAppError } from "../api/tauri";
import { useT } from "../i18n";

export type RouterContext = { queryClient: QueryClient };

export const Route = createRootRouteWithContext<RouterContext>()({
  component: () => (
    <AppShell>
      <Outlet />
    </AppShell>
  ),
  errorComponent: ({ error, reset }) => (
    <div className="px-gutter pt-[calc(var(--spacing-nav)+36px)]">
      <ErrorState error={toAppError(error)} onRetry={reset} />
    </div>
  ),
  notFoundComponent: NotFound,
});

function NotFound() {
  const t = useT();
  return <PageStub title={t.notFound.title}>{t.notFound.body}</PageStub>;
}
