import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

export type Cell =
  | { t: "text"; v: string }
  | { t: "int"; v: number }
  | { t: "time"; v: number }
  | { t: "null" };

export interface Row {
  values: Record<string, Cell>;
  raw: [string, string][];
}

export interface CatMeta {
  name: string;
  count: number;
}

export interface Overview {
  evidence: string;
  browser: string;
  kind: string;
  sha256: string;
  path: string;
  size: number;
  modified: number | null;
  version: string | null;
  installed: number | null;
  notes: string[];
  categories: CatMeta[];
}

export interface ViewData {
  columns: string[];
  rows: Row[];
}

export interface Category {
  name: string;
  columns: string[];
  rows: Row[];
}

export interface LoadSummary {
  loaded: number;
  errors: string[];
}

export interface SubItem {
  label: string;
  count: number;
  kind: "cat" | "flagged";
  ai: number | null;
  ci: number | null;
  browser: string | null;
}
export interface SubGroup {
  name: string;
  items: SubItem[];
}
export interface BrowserGroup {
  browser: string;
  subs: SubGroup[];
}

export interface TagEntry {
  sig: string;
  label: string;
  color: string;
  note: string;
  context: string;
  summary: string;
  when: string;
  browser: string;
  path: string;
  modified: number | null;
  size: number | null;
  origin?: string;
}

export interface CaseInfo {
  numero: string;
  examinateur: string;
  service: string;
  notes: string;
}

export interface ReportOptions {
  only_tagged: boolean;
  include_tags: boolean;
  include_stats: boolean;
  include_histogram: boolean;
  include_timeline: boolean;
  include_domains: boolean;
  include_terms: boolean;
  include_ga: boolean;
  include_online: boolean;
  include_flagged: boolean;
  include_details: boolean;
  include_identity: boolean;
  include_downloads: boolean;
  include_geolocation: boolean;
  include_vpn: boolean;
  include_recovered: boolean;
  include_crypto: boolean;
  include_ioc: boolean;
  include_sync: boolean;
  include_extensions: boolean;
  include_heatmap: boolean;
  include_map: boolean;
}

export interface KwItem {
  word: string;
  list: string;
}

export interface MediaItem {
  name: string;
  mime: string;
  size: number;
  data: string; // base64
}

export interface HtmlResource {
  kind: string;
  url: string;
}
export interface HtmlItem {
  name: string;
  title: string;
  size: number;
  source: string;
  resources: HtmlResource[];
}

export const api = {
  addPaths: (paths: string[]) => invoke<LoadSummary>("add_paths", { paths }),
  addFolder: (folder: string) => invoke<LoadSummary>("add_folder", { folder }),
  autodetect: () => invoke<LoadSummary>("autodetect"),
  version: () => invoke<string>("version"),
  cacheMedia: (folder: string) => invoke<MediaItem[]>("cache_media", { folder }),
  saveMedia: (path: string, data: string) =>
    invoke<void>("save_media", { path, data }),
  cacheHtml: (folder: string) => invoke<HtmlItem[]>("cache_html", { folder }),
  addCache: (folder: string) => invoke<LoadSummary>("add_cache", { folder }),
  clear: () => invoke<void>("clear"),
  overview: () => invoke<Overview[]>("overview"),
  setBrowser: (ai: number, browser: string) =>
    invoke<void>("set_browser", { ai, browser }),
  category: (ai: number, ci: number) =>
    invoke<Category | null>("category", { ai, ci }),
  timeline: (noisy: boolean) => invoke<ViewData>("timeline", { noisy }),
  domains: (noisy: boolean) => invoke<ViewData>("domains", { noisy }),
  search: (term: string) => invoke<ViewData>("search", { term }),
  browserGroups: () => invoke<BrowserGroup[]>("browser_groups"),
  flagged: (browser: string | null) =>
    invoke<ViewData>("flagged", { browser }),
  searchTerms: () => invoke<ViewData>("search_terms"),
  stats: () => invoke<ViewData>("stats"),
  gaCookies: () => invoke<ViewData>("ga_cookies"),
  histogram: () => invoke<ViewData>("histogram"),
  onlineServices: () => invoke<ViewData>("online_services"),
  keywordSearch: (keywords: KwItem[]) =>
    invoke<ViewData>("keyword_search", { keywords }),
  identity: () => invoke<ViewData>("identity"),
  downloads: () => invoke<ViewData>("downloads"),
  geolocation: () => invoke<ViewData>("geolocation"),
  vpn: () => invoke<ViewData>("vpn"),
  recovered: () => invoke<ViewData>("recovered"),
  crypto: () => invoke<ViewData>("crypto"),
  kwResults: () => invoke<ViewData>("kw_results_view"),
  iocLoad: (path: string) => invoke<ViewData>("ioc_load", { path }),
  iocResults: () => invoke<ViewData>("ioc_results_view"),
  resultsCounts: () => invoke<Record<string, number>>("results_counts"),
  chromiumPasswords: () => invoke<ViewData>("chromium_passwords"),
  firefoxPasswords: (masterPassword: string, dir?: string | null) =>
    invoke<ViewData>("firefox_passwords", { masterPassword, dir: dir ?? null }),
  syncAccounts: () => invoke<ViewData>("sync_accounts"),
  extensions: () => invoke<ViewData>("extensions"),
  geopoints: () => invoke<ViewData>("geopoints"),
  heatmap: (tz: string) => invoke<ViewData>("heatmap", { tz }),
  loadE01: (path: string) => invoke<LoadSummary>("load_e01", { path }),
  openUrl: (url: string) => openUrl(url),
  sha256Of: (path: string) => invoke<string>("sha256_of", { path }),
  setTag: (entry: TagEntry) => invoke<void>("set_tag", { entry }),
  setTags: (entries: TagEntry[]) => invoke<void>("set_tags", { entries }),
  removeTag: (sig: string) => invoke<void>("remove_tag", { sig }),
  removeTags: (sigs: string[]) => invoke<void>("remove_tags", { sigs }),
  listTags: () => invoke<TagEntry[]>("list_tags"),
  saveTags: (path: string) => invoke<void>("save_tags", { path }),
  loadTags: (path: string) => invoke<TagEntry[]>("load_tags", { path }),
  exportCategory: (
    ai: number,
    ci: number,
    path: string,
    tz: string,
    format: string
  ) => invoke<void>("export_category", { ai, ci, path, tz, format }),
  exportView: (
    view: string,
    term: string | null,
    noisy: boolean,
    path: string,
    tz: string,
    format: string
  ) => invoke<void>("export_view", { view, term, noisy, path, tz, format }),
  exportReport: (
    path: string,
    tz: string,
    noisy: boolean,
    caseInfo: CaseInfo,
    options: ReportOptions,
    identityPick: Record<string, string>[] = []
  ) =>
    invoke<void>("export_report", {
      path,
      tz,
      noisy,
      case: caseInfo,
      options,
      identityPick,
    }),
  exportReportPdf: (
    path: string,
    tz: string,
    noisy: boolean,
    caseInfo: CaseInfo,
    options: ReportOptions,
    identityPick: Record<string, string>[] = []
  ) =>
    invoke<void>("export_report_pdf", {
      path,
      tz,
      noisy,
      case: caseInfo,
      options,
      identityPick,
    }),
};

export function fmtCell(cell: Cell | undefined, tz: string): string {
  if (!cell || cell.t === "null") return "";
  if (cell.t === "text") return cell.v;
  if (cell.t === "int") return String(cell.v);
  // time : millisecondes Unix UTC
  const d = new Date(cell.v);
  if (isNaN(d.getTime())) return "";
  if (tz === "Local") {
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(
      d.getHours()
    )}:${p(d.getMinutes())}:${p(d.getSeconds())} (local)`;
  }
  return d.toISOString().replace("T", " ").slice(0, 19) + " UTC";
}

export function cellSortKey(cell: Cell | undefined): string | number {
  if (!cell || cell.t === "null") return "";
  if (cell.t === "time" || cell.t === "int") return cell.v;
  return cell.v.toLowerCase();
}
