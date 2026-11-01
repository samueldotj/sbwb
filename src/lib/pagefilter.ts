// The "unreviewed pages only" filter. A page is unreviewed while it is in the
// processing scope and has no current approval; an approval outdated by a later
// change needs a fresh look, so it counts as unreviewed too.
import type { PageRow } from "$lib/api";

export function isUnreviewed(p: PageRow): boolean {
  return p.status !== "unprocessed" && p.status !== "excluded" && p.approval !== "current";
}

/** The pages to list. `keep` stays listed whatever its state, so a page does
 *  not vanish from under the reviewer the moment it is approved. */
export function filterPages(pages: PageRow[], unreviewedOnly: boolean, keep: number | null = null): PageRow[] {
  if (!unreviewedOnly) return pages;
  return pages.filter((p) => p.index === keep || isUnreviewed(p));
}

/** The nearest unreviewed page after (`dir` 1) or before (`dir` -1) `from`. */
export function nearestUnreviewed(pages: PageRow[], from: number, dir: -1 | 1): number | null {
  for (let i = from + dir; i >= 0 && i < pages.length; i += dir) {
    const p = pages[i];
    if (p && isUnreviewed(p)) return i;
  }
  return null;
}
