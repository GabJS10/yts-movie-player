import type { SVGProps } from "react";

// Authored 24px stroke icons, one weight (from the design prototype).
const PATHS = {
  play: (
    <path
      d="M7 4.5v15a1 1 0 0 0 1.5.86l12.4-7.5a1 1 0 0 0 0-1.72L8.5 3.64A1 1 0 0 0 7 4.5Z"
      fill="currentColor"
      stroke="none"
    />
  ),
  search: (
    <>
      <circle cx="10.5" cy="10.5" r="6.5" />
      <path d="m20 20-4.6-4.6" />
    </>
  ),
  download: <path d="M12 3.5v11.5M7 10.5l5 5 5-5M4.5 20h15" />,
  settings: (
    <>
      <path d="M4 7h9M17 7h3M4 17h3M11 17h9" />
      <circle cx="15" cy="7" r="2.2" />
      <circle cx="9" cy="17" r="2.2" />
    </>
  ),
  home: <path d="M4 10.5 12 4l8 6.5V19a1 1 0 0 1-1 1h-4.5v-6h-5v6H5a1 1 0 0 1-1-1Z" />,
  heart: (
    <path d="M12 20s-7.5-4.6-9.2-9.4C1.7 7.4 3.8 4 7.2 4c2 0 3.6 1.1 4.8 2.8C13.2 5.1 14.8 4 16.8 4c3.4 0 5.5 3.4 4.4 6.6C19.5 15.4 12 20 12 20Z" />
  ),
  "heart-fill": (
    <path
      d="M12 20s-7.5-4.6-9.2-9.4C1.7 7.4 3.8 4 7.2 4c2 0 3.6 1.1 4.8 2.8C13.2 5.1 14.8 4 16.8 4c3.4 0 5.5 3.4 4.4 6.6C19.5 15.4 12 20 12 20Z"
      fill="currentColor"
    />
  ),
  check: <path d="m5 12.5 4.5 4.5L19 7.5" />,
  cc: (
    <>
      <rect x="3" y="5" width="18" height="14" rx="2" />
      <path d="M10.5 10.2a2.3 2.3 0 1 0 0 3.6M17 10.2a2.3 2.3 0 1 0 0 3.6" />
    </>
  ),
  upload: <path d="M12 15.5V4M7 9l5-5 5 5M4.5 20h15" />,
  minus: <path d="M5 12h14" />,
  eye: (
    <>
      <path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12Z" />
      <circle cx="12" cy="12" r="3" />
    </>
  ),
  trash: (
    <path d="M4.5 7h15M9.5 7V4.5h5V7M6.5 7l.8 12a1.5 1.5 0 0 0 1.5 1.4h6.4a1.5 1.5 0 0 0 1.5-1.4l.8-12" />
  ),
  folder: (
    <path d="M3.5 7a1.5 1.5 0 0 1 1.5-1.5h4.2l2 2H19A1.5 1.5 0 0 1 20.5 9v9A1.5 1.5 0 0 1 19 19.5H5A1.5 1.5 0 0 1 3.5 18Z" />
  ),
  "arrow-up": <path d="M12 19V5M6 11l6-6 6 6" />,
  down: <path d="M12 4v16M6 14l6 6 6-6" />,
  up: <path d="M12 20V4M6 10l6-6 6 6" />,
  back: <path d="M19 12H5M11 6l-6 6 6 6" />,
  "chev-r": <path d="m9 5 7 7-7 7" />,
  "chev-l": <path d="m15 5-7 7 7 7" />,
  info: (
    <>
      <circle cx="12" cy="12" r="9.5" />
      <path d="M12 11v6M12 7.5v.01" />
    </>
  ),
  plus: <path d="M12 5v14M5 12h14" />,
  alert: (
    <>
      <path d="M12 8.5v5M12 16.5v.01" />
      <path d="M10.3 4.2 2.8 17.5A2 2 0 0 0 4.5 20.5h15a2 2 0 0 0 1.7-3L13.7 4.2a2 2 0 0 0-3.4 0Z" />
    </>
  ),
  refresh: (
    <>
      <path d="M19.5 12a7.5 7.5 0 1 1-2.6-5.7" />
      <path d="M19.5 4v4.5H15" />
    </>
  ),
  star: (
    <path
      d="m12 3 2.7 5.6 6.1.8-4.5 4.2 1.1 6.1L12 16.8l-5.4 2.9 1.1-6.1-4.5-4.2 6.1-.8Z"
      fill="currentColor"
      stroke="none"
    />
  ),
  x: <path d="M6 6l12 12M18 6 6 18" />,
  pause: (
    <>
      <rect x="6" y="4" width="4" height="16" rx="1" fill="currentColor" stroke="none" />
      <rect x="14" y="4" width="4" height="16" rx="1" fill="currentColor" stroke="none" />
    </>
  ),
  rew10: (
    <>
      <path d="M4.5 12a7.5 7.5 0 1 0 2.2-5.3" />
      <path d="M4 3.5v4h4" />
      <text
        x="12"
        y="15.2"
        textAnchor="middle"
        fontSize="7.5"
        fontWeight="800"
        fill="currentColor"
        stroke="none"
      >
        10
      </text>
    </>
  ),
  fwd10: (
    <>
      <path d="M19.5 12a7.5 7.5 0 1 1-2.2-5.3" />
      <path d="M20 3.5v4h-4" />
      <text
        x="12"
        y="15.2"
        textAnchor="middle"
        fontSize="7.5"
        fontWeight="800"
        fill="currentColor"
        stroke="none"
      >
        10
      </text>
    </>
  ),
  volume: (
    <>
      <path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4Z" />
      <path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11" />
    </>
  ),
  mute: (
    <>
      <path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4Z" />
      <path d="m16 9.5 5 5M21 9.5l-5 5" />
    </>
  ),
  fullscreen: <path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" />,
  "fullscreen-exit": <path d="M9 4v5H4M15 4v5h5M9 20v-5H4M15 20v-5h5" />,
  external: <path d="M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5" />,
} as const;

export type IconName = keyof typeof PATHS;

type Props = { name: IconName; size?: number } & Omit<SVGProps<SVGSVGElement>, "name">;

export function Icon({ name, size = 20, ...rest }: Props) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      className="flex-none"
      {...rest}
    >
      {PATHS[name]}
    </svg>
  );
}
