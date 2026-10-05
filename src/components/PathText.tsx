import { splitPathTail } from "../lib/paths";

/**
 * A folder path on one line that, when it doesn't fit, cuts the middle and keeps the last folder:
 * "C:\Users\ana\AppDa…\library". The full path is in the tooltip and in what screen readers announce.
 */
export function PathText({ path, className = "" }: { path: string; className?: string }) {
  const [head, tail] = splitPathTail(path);
  return (
    <span className={`flex min-w-0 font-mono ${className}`} title={path} data-path={path}>
      <span className="sr-only">{path}</span>
      <span className="min-w-0 truncate" aria-hidden="true">
        {head}
      </span>
      <span className="max-w-full flex-none truncate" aria-hidden="true">
        {tail}
      </span>
    </span>
  );
}
