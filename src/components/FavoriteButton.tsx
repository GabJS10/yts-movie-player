import { useToggleFavorite } from "../api/queries";
import type { MovieSummary } from "../api/types";
import { Icon } from "./Icon";

type Props = {
  movie: MovieSummary;
  /** From get_movie (optimistically updated by the toggle); null while it loads. */
  isFavorite: boolean | null;
  /** "full": labelled outline button (movie page). "round": icon-only ghost button (hero). */
  variant?: "full" | "round";
  testId?: string;
};

/** ♥ Mi lista: optimistic, rolled back with a toast if the backend fails. */
export function FavoriteButton({ movie, isFavorite, variant = "full", testId }: Props) {
  const toggle = useToggleFavorite();
  const on = isFavorite === true;
  const onClick = () => {
    if (isFavorite === null || toggle.isPending) return;
    toggle.mutate({ movie, on: !on });
  };
  const icon = (
    <Icon name={on ? "heart-fill" : "heart"} size={22} className={on ? "text-green" : undefined} />
  );

  if (variant === "round") {
    return (
      <button
        type="button"
        className="btn btn-ghost w-12 rounded-full p-0"
        aria-pressed={on}
        aria-label={on ? "Quitar de Mi lista" : "Añadir a Mi lista"}
        data-testid={testId}
        title={on ? "En Mi lista" : "Añadir a Mi lista"}
        disabled={isFavorite === null}
        onClick={onClick}
      >
        {icon}
      </button>
    );
  }
  return (
    <button
      type="button"
      className="btn btn-line"
      aria-pressed={on}
      data-testid={testId}
      disabled={isFavorite === null}
      onClick={onClick}
    >
      {icon}
      {on ? "En Mi lista" : "Mi lista"}
    </button>
  );
}
