// Commandes du cœur Rust (desktop/app/src-tauri/src/commands.rs).
import { invoke } from "@tauri-apps/api/core";

export interface Row {
  id: string;
  ids: string[];
  title: string;
  authorLine: string;
  formats: string[];
  cover: string | null;
  series: string | null;
  seriesIndex: number | null;
  addedAt: number;
  lastSentAt: number | null;
  pending: boolean;
  trashed: boolean;
  unavailable: boolean;
  drm: boolean;
}

export interface Entry {
  key: string;
  name: string;
  count: number;
  unavailable: boolean;
}

export interface Sidebar {
  counts: Record<string, number>;
  folders: Entry[];
  formats: Entry[];
  authors: Entry[];
  series: Entry[];
  shelves: Entry[];
  device: Entry | null;
}

export interface Edits {
  title: string;
  authors: string[];
  publisher?: string | null;
  language?: string | null;
  summary?: string | null;
  publishedDate?: string | null;
  isbn?: string | null;
  series?: string | null;
  seriesIndex?: number | null;
  genre?: string | null;
}

export interface FileInfo {
  id: string;
  format: string;
  path: string;
  size: number;
  hiddenCopy: boolean;
}

export interface Detail {
  id: string;
  ids: string[];
  edits: Edits;
  tags: string[];
  cover: string | null;
  files: FileInfo[];
  folder: string | null;
  drm: boolean;
  trashed: boolean;
  unavailable: boolean;
  pending: boolean;
  lastSentAt: number | null;
  addedAt: number;
  sendNote: "drm" | "azw3" | "epub" | null;
  canSend: boolean;
}

export interface DeviceDocument {
  id: string;
  name: string;
  title: string;
  authorLine: string;
  format: string;
  size: number;
  cover: string | null;
  drm: boolean;
}

export interface DeviceInfo {
  id: string;
  name: string;
  brand: string;
  freeSpace: number | null;
  documents: DeviceDocument[];
  missing: Row[];
}

export interface SendResult {
  id: string;
  title: string;
  ok: boolean;
  message: string | null;
}

export interface SettingsInfo {
  language: string | null;
  showDuplicates: boolean;
  checkUpdates: boolean;
  hiddenDuplicates: number;
  coversBytes: number;
  conversionsBytes: number;
  dataFolder: string;
  version: string;
  folders: Entry[];
  readers: Entry[];
  startupError: string | null;
}

export interface UpdateInfo {
  version: string;
  /** Installeur Windows, sinon page de la version sur GitHub. */
  url: string;
}

export const api = {
  list: (page: string, query: string, sort: string) => invoke<Row[]>("list", { page, query, sort }),
  sidebar: () => invoke<Sidebar>("sidebar"),
  detail: (ids: string[]) => invoke<Detail | null>("detail", { ids }),
  saveBook: (ids: string[], edits: Edits, tags: string[]) => invoke<void>("save_book", { ids, edits, tags }),
  rescan: () => invoke<void>("rescan"),
  addFolder: (path: string) => invoke<void>("add_folder", { path }),
  removeFolder: (path: string) => invoke<void>("remove_folder", { path }),
  import: (paths: string[]) => invoke<number>("import", { paths }),
  trash: (ids: string[]) => invoke<void>("trash", { ids }),
  restore: (ids: string[]) => invoke<void>("restore", { ids }),
  deletePermanently: (ids: string[]) => invoke<void>("delete_permanently", { ids }),
  emptyTrash: () => invoke<void>("empty_trash"),
  queue: (ids: string[]) => invoke<void>("queue", { ids }),
  unqueue: (ids: string[]) => invoke<void>("unqueue", { ids }),
  forgetSent: (ids: string[]) => invoke<void>("forget_sent", { ids }),
  device: () => invoke<DeviceInfo | null>("device"),
  send: (ids: string[]) => invoke<SendResult[]>("send", { ids }),
  eject: () => invoke<string>("eject"),
  deleteDocument: (id: string) => invoke<void>("delete_document", { id }),
  renameReader: (id: string, name: string) => invoke<void>("rename_reader", { id, name }),
  forgetReader: (id: string) => invoke<void>("forget_reader", { id }),
  settings: () => invoke<SettingsInfo>("settings"),
  setLanguage: (language: string | null) => invoke<void>("set_language", { language }),
  setShowDuplicates: (show: boolean) => invoke<void>("set_show_duplicates", { show }),
  setCheckUpdates: (check: boolean) => invoke<void>("set_check_updates", { check }),
  checkUpdate: (force: boolean) => invoke<UpdateInfo | null>("check_update", { force }),
  clearConversions: () => invoke<void>("clear_conversions"),
  exportEpub: (id: string, target: string) => invoke<void>("export_epub", { id, target }),
};

/** Message d'une erreur rendue par une commande. */
export function message(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}
