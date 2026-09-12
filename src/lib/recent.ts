// Recent books list, kept in app settings, separate from project content
// (PRJ-05). Clearing it never deletes projects.

import type { ProjectSummary } from "./api";
import { isTauri } from "./ipc";

export type RecentEntry = {
  path: string;
  title: string;
  volume: string | null;
  sourceName: string;
  sourcePages: number;
  inScope: number;
  done: number;
  approved: number;
  failed: number;
  openedAt: string;
};

const FILE = "recent.json";
const KEY = "books";
const MAX = 12;

async function store() {
  const { load } = await import("@tauri-apps/plugin-store");
  return load(FILE, { autoSave: true, defaults: {} });
}

function entryFrom(s: ProjectSummary): RecentEntry {
  const m = s.meta;
  const title = m.title ?? m.source.title ?? m.source.name.replace(/\.pdf$/i, "");
  return {
    path: s.path,
    title,
    volume: null,
    sourceName: m.source.name,
    sourcePages: m.source.page_count,
    inScope: s.counts.in_scope,
    done: s.counts.done,
    approved: s.counts.approved,
    failed: s.counts.failed,
    openedAt: new Date().toISOString(),
  };
}

export const recent = {
  async list(): Promise<RecentEntry[]> {
    if (!isTauri) return [];
    const st = await store();
    const v = (await st.get<RecentEntry[]>(KEY)) ?? [];
    return v;
  },
  async remember(s: ProjectSummary) {
    if (!isTauri) return;
    const st = await store();
    const cur = ((await st.get<RecentEntry[]>(KEY)) ?? []).filter((e) => e.path !== s.path);
    cur.unshift(entryFrom(s));
    await st.set(KEY, cur.slice(0, MAX));
  },
  async forget(path: string) {
    if (!isTauri) return;
    const st = await store();
    const cur = ((await st.get<RecentEntry[]>(KEY)) ?? []).filter((e) => e.path !== path);
    await st.set(KEY, cur);
  },
  async clear() {
    if (!isTauri) return;
    const st = await store();
    await st.set(KEY, []);
  },
};
