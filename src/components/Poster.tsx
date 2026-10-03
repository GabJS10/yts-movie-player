import { useState } from "react";

type Props = {
  src: string | null;
  title: string;
  className?: string;
  eager?: boolean;
};

/** Poster image; when YTS has no cover (null) or it fails to load, a typographic placeholder. */
export function Poster({ src, title, className = "", eager = false }: Props) {
  const [failed, setFailed] = useState(false);
  if (!src || failed) {
    return (
      <div
        className={`flex size-full items-end bg-surface p-3 shadow-[inset_0_0_0_1px_var(--color-line)] ${className}`}
        data-placeholder
      >
        <span className="line-clamp-4 text-[15px] leading-tight font-[850] text-text-2 uppercase stretch-condensed">
          {title}
        </span>
      </div>
    );
  }
  return (
    <img
      src={src}
      alt=""
      loading={eager ? "eager" : "lazy"}
      decoding="async"
      width={230}
      height={345}
      onError={() => setFailed(true)}
      className={`size-full object-cover ${className}`}
    />
  );
}
