import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { open, save, message, ask } from "@tauri-apps/plugin-dialog";
import {
  api,
  cellSortKey,
  fmtCell,
  type BrowserGroup,
  type CaseInfo,
  type HtmlItem,
  type KwItem,
  type MediaItem,
  type Overview,
  type ReportOptions,
  type Row,
  type SubItem,
  type TagEntry,
} from "./api";
import L from "leaflet";
import "leaflet/dist/leaflet.css";
import markerIcon2x from "leaflet/dist/images/marker-icon-2x.png";
import markerIcon from "leaflet/dist/images/marker-icon.png";
import markerShadow from "leaflet/dist/images/marker-shadow.png";

// Corrige les icônes de marqueur avec Vite (chemins d'assets bundlés).
L.Icon.Default.mergeOptions({
  iconRetinaUrl: markerIcon2x,
  iconUrl: markerIcon,
  shadowUrl: markerShadow,
});

type GeoPoint = { lat: number; lon: number; label: string; url: string };

/** Carte Leaflet (tuiles OpenStreetMap, en ligne) situant tous les points. */
function GeoMap({ points, onOpen }: { points: GeoPoint[]; onOpen: (u: string) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const mapRef = useRef<L.Map | null>(null);
  const layerRef = useRef<L.LayerGroup | null>(null);
  useEffect(() => {
    if (!ref.current) return;
    if (!mapRef.current) {
      mapRef.current = L.map(ref.current);
      L.tileLayer("https://tile.openstreetmap.org/{z}/{x}/{y}.png", {
        maxZoom: 19,
        attribution: "© OpenStreetMap",
      }).addTo(mapRef.current);
      layerRef.current = L.layerGroup().addTo(mapRef.current);
    }
    const map = mapRef.current;
    const layer = layerRef.current!;
    layer.clearLayers();
    const latlngs: L.LatLngExpression[] = [];
    for (const p of points) {
      const esc = p.label.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c] as string));
      const m = L.marker([p.lat, p.lon]);
      m.bindPopup(
        `<b>${esc || "Point"}</b><br>${p.lat.toFixed(5)}, ${p.lon.toFixed(5)}` +
          (p.url ? `<br><a href="#" class="nf-osm">Ouvrir dans le navigateur</a>` : "")
      );
      m.on("popupopen", (e) => {
        const a = (e.popup.getElement() as HTMLElement | null)?.querySelector(".nf-osm");
        if (a) a.addEventListener("click", (ev) => { ev.preventDefault(); onOpen(p.url); });
      });
      m.addTo(layer);
      latlngs.push([p.lat, p.lon]);
    }
    if (latlngs.length) map.fitBounds(latlngs as L.LatLngBoundsExpression, { padding: [30, 30], maxZoom: 14 });
    else map.setView([20, 0], 2);
    setTimeout(() => map.invalidateSize(), 60);
  }, [points, onOpen]);
  return (
    <div className="geomapwrap">
      <div ref={ref} className="geomap" />
      <div className="geomaphint">
        {points.length} point(s) · tuiles OpenStreetMap (connexion requise) · clic sur un repère pour le détail
      </div>
    </div>
  );
}

type TLEvent = { t: number; type: string };

const TL_PALETTE = ["#4285F4", "#DB4437", "#0F9D58", "#F4B400", "#AB47BC", "#00ACC1", "#FF7043", "#795548"];
const MOIS_FR = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];

/** Graduations de dates adaptées au zoom (années → mois → jours → heures). */
function genTicks(a: number, b: number, useUTC: boolean): { t: number; label: string }[] {
  const out: { t: number; label: string }[] = [];
  const span = b - a;
  const day = 86400000;
  const p2 = (n: number) => String(n).padStart(2, "0");
  const Y = (t: number) => (useUTC ? new Date(t).getUTCFullYear() : new Date(t).getFullYear());
  const Mo = (t: number) => (useUTC ? new Date(t).getUTCMonth() : new Date(t).getMonth());
  const Da = (t: number) => (useUTC ? new Date(t).getUTCDate() : new Date(t).getDate());
  const Ho = (t: number) => (useUTC ? new Date(t).getUTCHours() : new Date(t).getHours());
  const Mi = (t: number) => (useUTC ? new Date(t).getUTCMinutes() : new Date(t).getMinutes());
  const mk = (y: number, mo: number, da: number, h = 0, mi = 0) =>
    useUTC ? Date.UTC(y, mo, da, h, mi) : new Date(y, mo, da, h, mi).getTime();

  if (span > 1095 * day) {
    const cand = [1, 2, 5, 10, 25, 50, 100];
    const y0 = Y(a), y1 = Y(b);
    let step = 1;
    for (const s of cand) { step = s; if ((y1 - y0) / s <= 8) break; }
    for (let y = Math.ceil(y0 / step) * step; mk(y, 0, 1) <= b; y += step) {
      const t = mk(y, 0, 1);
      if (t >= a) out.push({ t, label: String(y) });
    }
  } else if (span > 75 * day) {
    const cand = [1, 2, 3, 6];
    let step = 1;
    const tot = span / (30.44 * day);
    for (const s of cand) { step = s; if (tot / s <= 8) break; }
    let y = Y(a), mo = Math.ceil(Mo(a) / step) * step;
    while (mo > 11) { mo -= 12; y++; }
    for (let t = mk(y, mo, 1); t <= b; mo += step, (mo > 11 ? ((mo -= 12), y++) : 0), t = mk(y, mo, 1)) {
      if (t >= a) out.push({ t, label: `${MOIS_FR[((mo % 12) + 12) % 12]} ${y}` });
    }
  } else if (span > 2 * day) {
    const cand = [1, 2, 5, 10];
    let step = 1;
    const tot = span / day;
    for (const s of cand) { step = s; if (tot / s <= 9) break; }
    const mid = mk(Y(a), Mo(a), Da(a));
    for (let t = mid; t <= b; t += step * day) {
      if (t >= a) out.push({ t, label: `${p2(Da(t))}/${p2(Mo(t) + 1)}` });
    }
  } else {
    const cand = [60000, 5 * 60000, 15 * 60000, 30 * 60000, 3600000, 3 * 3600000, 6 * 3600000, 12 * 3600000];
    let step = 3600000;
    for (const s of cand) { step = s; if (span / s <= 9) break; }
    for (let t = Math.ceil(a / step) * step; t <= b; t += step) {
      const lbl = Ho(t) === 0 && Mi(t) === 0 ? `${p2(Da(t))}/${p2(Mo(t) + 1)}` : `${p2(Ho(t))}:${p2(Mi(t))}`;
      out.push({ t, label: lbl });
    }
  }
  return out;
}

/**
 * Timeline interactive (type Cellebrite / Google Trends) : courbes d'activité
 * par type d'évènement, axe de dates adaptatif au zoom, brosse de sélection
 * pour filtrer, zoom molette/boutons/double-clic jusqu'au jour et au-delà.
 */
function TimelineBrush({
  events,
  extent,
  win,
  sel,
  tz,
  onWin,
  onSel,
  onReset,
}: {
  events: TLEvent[];
  extent: [number, number];
  win: [number, number];
  sel: [number, number] | null;
  tz: string;
  onWin: (w: [number, number]) => void;
  onSel: (s: [number, number] | null) => void;
  onReset: () => void;
}) {
  const svgRef = useRef<SVGSVGElement>(null);
  const dragRef = useRef<{ t0: number; moved: boolean } | null>(null);
  const panRef = useRef<{ x: number; win: [number, number] } | null>(null);
  const [grab, setGrab] = useState(false);
  const [hover, setHover] = useState<{ frac: number; t: number; i: number } | null>(null);
  const N = 120;
  const [winMin, winMax] = win;
  const span = Math.max(1, winMax - winMin);
  const useUTC = tz === "UTC";
  // Au-delà d'un certain niveau de zoom (fenêtre ≤ 3 jours), on passe des
  // courbes lissées à des barres verticales, plus précises tranche par tranche.
  const barMode = span <= 3 * 86400000;

  // Types les plus fréquents (max 7 séries), + comptes par tranche.
  const { series, total, maxY } = useMemo(() => {
    const totByType = new Map<string, number>();
    for (const e of events) {
      if (e.t < winMin || e.t > winMax) continue;
      totByType.set(e.type, (totByType.get(e.type) || 0) + 1);
    }
    const top = [...totByType.entries()].sort((a, b) => b[1] - a[1]).slice(0, 7).map((x) => x[0]);
    const idx = new Map(top.map((ty, i) => [ty, i]));
    const ser = top.map(() => new Array(N).fill(0));
    const tot = new Array(N).fill(0);
    for (const e of events) {
      if (e.t < winMin || e.t > winMax) continue;
      let i = Math.floor(((e.t - winMin) / span) * N);
      if (i >= N) i = N - 1;
      if (i < 0) i = 0;
      tot[i]++;
      const si = idx.get(e.type);
      if (si !== undefined) ser[si][i]++;
    }
    let my = 1;
    for (const v of tot) if (v > my) my = v;
    return { series: top.map((ty, i) => ({ type: ty, counts: ser[i] })), total: tot, maxY: my };
  }, [events, winMin, winMax, span]);

  const ticks = useMemo(() => genTicks(winMin, winMax, useUTC), [winMin, winMax, useUTC]);

  const fmt = (ms: number) => {
    const d = new Date(ms);
    const p = (n: number) => String(n).padStart(2, "0");
    return useUTC
      ? `${p(d.getUTCDate())}-${p(d.getUTCMonth() + 1)}-${d.getUTCFullYear()} ${p(d.getUTCHours())}:${p(d.getUTCMinutes())}Z`
      : `${p(d.getDate())}-${p(d.getMonth() + 1)}-${d.getFullYear()} ${p(d.getHours())}:${p(d.getMinutes())}`;
  };
  const xCenter = (i: number) => (i + 0.5) * (1000 / N);
  const yOf = (c: number) => 90 - (c / maxY) * 82;
  const path = (counts: number[]) =>
    counts.map((c, i) => `${i === 0 ? "M" : "L"}${xCenter(i).toFixed(1)},${yOf(c).toFixed(1)}`).join(" ");
  const areaPath = (counts: number[]) =>
    `M0,90 ` + counts.map((c, i) => `L${xCenter(i).toFixed(1)},${yOf(c).toFixed(1)}`).join(" ") + ` L1000,90 Z`;

  const fracFromClientX = (clientX: number) => {
    const rect = svgRef.current!.getBoundingClientRect();
    return Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
  };
  const timeAt = (clientX: number) => winMin + fracFromClientX(clientX) * span;
  const xOf = (t: number) => ((t - winMin) / span) * 1000;

  const updateHover = (clientX: number) => {
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect) return;
    const frac = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
    let i = Math.floor(frac * N);
    if (i >= N) i = N - 1;
    if (i < 0) i = 0;
    setHover({ frac, t: winMin + frac * span, i });
  };
  const onDown = (e: React.PointerEvent) => {
    (e.target as Element).setPointerCapture?.(e.pointerId);
    // clic droit (ou molette) = déplacement de la fenêtre (curseur main)
    if (e.button === 2 || e.button === 1) {
      panRef.current = { x: e.clientX, win: [winMin, winMax] };
      setGrab(true);
      return;
    }
    dragRef.current = { t0: timeAt(e.clientX), moved: false };
  };
  const onMove = (e: React.PointerEvent) => {
    if (panRef.current) {
      const rect = svgRef.current?.getBoundingClientRect();
      if (!rect) return;
      const dt = -((e.clientX - panRef.current.x) / rect.width) * span;
      let a = panRef.current.win[0] + dt;
      let b = panRef.current.win[1] + dt;
      if (a < extent[0]) { a = extent[0]; b = a + span; }
      if (b > extent[1]) { b = extent[1]; a = b - span; }
      onWin([a, b]);
      return;
    }
    if (dragRef.current) {
      const t1 = timeAt(e.clientX);
      dragRef.current.moved = true;
      onSel([Math.min(dragRef.current.t0, t1), Math.max(dragRef.current.t0, t1)]);
      return;
    }
    updateHover(e.clientX);
  };
  const onUp = () => {
    if (dragRef.current && !dragRef.current.moved) onSel(null);
    dragRef.current = null;
    panRef.current = null;
    setGrab(false);
  };

  useEffect(() => {
    const el = svgRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const center = winMin + fracFromClientX(e.clientX) * span;
      const frac = (center - winMin) / span;
      const factor = e.deltaY < 0 ? 0.75 : 1.33;
      const extSpan = extent[1] - extent[0];
      const newSpan = Math.min(extSpan, Math.max(60000, span * factor));
      let a = center - frac * newSpan;
      let b = a + newSpan;
      if (a < extent[0]) { a = extent[0]; b = a + newSpan; }
      if (b > extent[1]) { b = extent[1]; a = b - newSpan; }
      onWin([Math.max(extent[0], a), Math.min(extent[1], b)]);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [winMin, winMax, span, extent, onWin]);

  const selX1 = sel ? xOf(Math.max(sel[0], winMin)) : 0;
  const selX2 = sel ? xOf(Math.min(sel[1], winMax)) : 0;

  // barres empilées par série (mode zoom précis)
  const barW = (1000 / N) * 0.82;
  const bars: React.ReactNode[] = [];
  if (barMode) {
    for (let i = 0; i < N; i++) {
      const tt = total[i];
      if (!tt) continue;
      let acc = 0;
      const x = xCenter(i) - barW / 2;
      for (let si = 0; si < series.length; si++) {
        const c = series[si].counts[i];
        if (!c) continue;
        const yTop = yOf(acc + c);
        const h = Math.max(0.6, yOf(acc) - yTop);
        bars.push(
          <rect key={`${i}-${si}`} x={x.toFixed(1)} y={yTop.toFixed(1)} width={barW.toFixed(1)} height={h.toFixed(1)} className="tlbarrect" style={{ fill: TL_PALETTE[si % TL_PALETTE.length] }} />
        );
        acc += c;
      }
    }
  }

  // infobulle au survol
  const hoverInfo = hover
    ? {
        t: hover.t,
        tot: total[hover.i] || 0,
        parts: series
          .map((s, si) => ({ type: s.type, c: s.counts[hover.i] || 0, color: TL_PALETTE[si % TL_PALETTE.length] }))
          .filter((p) => p.c > 0),
      }
    : null;

  return (
    <div className="tlbrush">
      <div className="tlbar">
        <button className="btn small" disabled={!sel} onClick={() => { if (sel) { onWin([Math.max(extent[0], sel[0]), Math.min(extent[1], sel[1])]); onSel(null); } }}>🔍 Zoom sélection</button>
        <button className="btn small" onClick={() => {
          const c = (winMin + winMax) / 2; const extSpan = extent[1] - extent[0]; const ns = Math.min(extSpan, span * 2);
          let a = c - ns / 2, b = c + ns / 2;
          if (a < extent[0]) { a = extent[0]; b = a + ns; } if (b > extent[1]) { b = extent[1]; a = b - ns; }
          onWin([Math.max(extent[0], a), Math.min(extent[1], b)]);
        }}>− Dézoomer</button>
        <button className="btn small" onClick={onReset}>⤢ Tout afficher</button>
        <span className="tlinfo">Fenêtre : <b>{fmt(winMin)}</b> → <b>{fmt(winMax)}</b></span>
        {sel && <span className="tlsel">Sélection : {fmt(sel[0])} → {fmt(sel[1])}</span>}
        <span className="tllegend">
          {series.map((s, i) => (
            <span key={i} className="tllg"><i style={{ background: TL_PALETTE[i % TL_PALETTE.length] }} />{s.type}</span>
          ))}
        </span>
      </div>
      <div className="tlsvgwrap">
      <svg
        ref={svgRef}
        className="tlsvg"
        viewBox="0 0 1000 100"
        preserveAspectRatio="none"
        style={{ cursor: grab ? "grabbing" : "crosshair" }}
        onPointerDown={onDown}
        onPointerMove={onMove}
        onPointerUp={onUp}
        onPointerLeave={() => setHover(null)}
        onContextMenu={(e) => e.preventDefault()}
        onDoubleClick={(e) => {
          const center = timeAt(e.clientX);
          const ns = Math.max(60000, span / 4);
          const a = Math.max(extent[0], center - ns / 2);
          const b = Math.min(extent[1], a + ns);
          onWin([a, b]);
        }}
      >
        <rect x={0} y={0} width={1000} height={100} className="tlbg" />
        {ticks.map((tk, i) => {
          const x = xOf(tk.t);
          return <line key={i} x1={x} y1={0} x2={x} y2={90} className="tlgrid" />;
        })}
        {sel && selX2 > selX1 && <rect x={selX1} y={0} width={selX2 - selX1} height={90} className="tlselrect" />}
        {barMode ? (
          bars
        ) : (
          <>
            <path d={areaPath(total)} className="tlarea" />
            {series.map((s, i) => (
              <path key={i} d={path(s.counts)} className="tlline" style={{ stroke: TL_PALETTE[i % TL_PALETTE.length] }} />
            ))}
          </>
        )}
        {hover && <line x1={(hover.frac * 1000).toFixed(1)} y1={0} x2={(hover.frac * 1000).toFixed(1)} y2={90} className="tlcursor" />}
        <line x1={0} y1={90} x2={1000} y2={90} className="tlaxis" />
      </svg>
      {hoverInfo && (
        <div
          className="tltip"
          style={{ left: `${(hover!.frac * 100).toFixed(1)}%` }}
        >
          <div className="tltipd">{fmt(hoverInfo.t)}</div>
          <div className="tltipn"><b>{hoverInfo.tot}</b> consultation{hoverInfo.tot > 1 ? "s" : ""}</div>
          {hoverInfo.parts.length > 1 &&
            hoverInfo.parts.slice(0, 6).map((p, k) => (
              <div key={k} className="tltipr"><i style={{ background: p.color }} />{p.type} : {p.c}</div>
            ))}
        </div>
      )}
      </div>
      <div className="tlticks">
        {ticks.map((tk, i) => (
          <span key={i} className="tltick" style={{ left: (xOf(tk.t) / 10).toFixed(2) + "%" }}>{tk.label}</span>
        ))}
      </div>
      <div className="tlhint">glisser = sélectionner · clic droit = déplacer · molette = zoomer · double-clic = affiner{barMode ? " · mode barres (zoom précis)" : ""} · {events.length} évènements</div>
    </div>
  );
}

type Selection =
  | { kind: "cat"; ai: number; ci: number }
  | { kind: "timeline" }
  | { kind: "domains" }
  | { kind: "search"; term: string }
  | { kind: "flagged"; browser: string | null }
  | { kind: "search_terms" }
  | { kind: "stats" }
  | { kind: "ga" }
  | { kind: "histogram" }
  | { kind: "online" }
  | { kind: "keywords" }
  | { kind: "identity"; group?: string; type?: string }
  | { kind: "downloads" }
  | { kind: "geolocation" }
  | { kind: "vpn" }
  | { kind: "recovered" }
  | { kind: "crypto" }
  | { kind: "kwresults" }
  | { kind: "fxpasswords" }
  | { kind: "chrpasswords"; browser: string }
  | { kind: "ioc" }
  | { kind: "sync" }
  | { kind: "extensions" }
  | { kind: "geopoints" }
  | { kind: "heatmap" }
  | { kind: "media" }
  | { kind: "html" }
  | { kind: "tags" };

interface KwList {
  name: string;
  words: string[];
}

const BROWSER_PRESETS = [
  "Google Chrome",
  "Microsoft Edge",
  "Brave",
  "Opera",
  "Vivaldi",
  "Chromium",
  "Firefox",
  "Safari",
  "Internet Explorer / Edge Legacy",
];

const DEFAULT_REPORT_OPTS: ReportOptions = {
  only_tagged: false,
  include_tags: true,
  include_stats: true,
  include_histogram: true,
  include_timeline: true,
  include_domains: true,
  include_terms: true,
  include_ga: true,
  include_online: true,
  include_flagged: true,
  include_details: true,
  include_identity: true,
  include_downloads: true,
  include_geolocation: true,
  include_vpn: true,
  include_recovered: true,
  include_crypto: true,
  include_ioc: true,
  include_sync: true,
  include_extensions: true,
  include_heatmap: true,
  include_map: true,
};

// listes de mots-clés forensiques par défaut (l'utilisateur peut les éditer)
const DEFAULT_KW_LISTS: KwList[] = [
  { name: "Anonymat / Dark web", words: ["onion", "tor browser", "tails", "i2p", "vpn", "proxy", "monero", "mixer", "tumbler", "dread", "hidden service"] },
  { name: "Stockage & partage", words: ["mega.nz", "dropbox", "wetransfer", "torrent", "magnet:", "utorrent", "qbittorrent", "uptobox", "1fichier", "rapidgator"] },
  { name: "Piratage / outils", words: ["crack", "keygen", "serial", "warez", "nulled", "exploit", "malware", "keylogger", "phishing", "metasploit", "cobalt strike"] },
  { name: "Paiement / fraude", words: ["carding", "cvv", "fullz", "bank log", "skimmer", "dumps", "western union"] },
  { name: "Messageries chiffrées", words: ["signal", "telegram", "wickr", "threema", "session messenger", "protonmail", "tutanota"] },
  { name: "Armes", words: ["arme", "pistolet", "munition", "silencieux", "glock", "kalachnikov"] },
  { name: "Stupéfiants", words: ["cannabis", "cocaine", "mdma", "ecstasy", "heroine", "lsd", "amphetamine", "stupefiant"] },
];

function loadKwLists(): KwList[] {
  try {
    const raw = localStorage.getItem("nf_kwlists");
    if (raw) return JSON.parse(raw);
  } catch {
    /* ignore */
  }
  return DEFAULT_KW_LISTS;
}

interface View {
  title: string;
  sha: string;
  columns: string[];
  rows: Row[];
}

const THEMES = ["sombre", "clair", "contraste"] as const;
const TAG_PRESETS: { name: string; color: string }[] = [
  { name: "À retenir", color: "#f59e0b" },
  { name: "Preuve", color: "#ef4444" },
  { name: "Suspect", color: "#a855f7" },
  { name: "À écarter", color: "#64748b" },
  { name: "À vérifier", color: "#3b82f6" },
];

// --- Référentiel desktop (Windows Vista→11, macOS, Linux). Pour Windows XP,
//     remplacer %LOCALAPPDATA% par %USERPROFILE%\Local Settings\Application Data
//     et %APPDATA% par %USERPROFILE%\Application Data. ---
const HELP_LOCATIONS: { b: string; win: string; other: string }[] = [
  {
    b: "Chrome",
    win: "%LOCALAPPDATA%\\Google\\Chrome\\User Data\\Default",
    other: "macOS : ~/Library/Application Support/Google/Chrome/Default · Linux : ~/.config/google-chrome/Default",
  },
  {
    b: "Edge (Chromium)",
    win: "%LOCALAPPDATA%\\Microsoft\\Edge\\User Data\\Default",
    other: "macOS : ~/Library/Application Support/Microsoft Edge/Default · Linux : ~/.config/microsoft-edge/Default",
  },
  {
    b: "Brave",
    win: "%LOCALAPPDATA%\\BraveSoftware\\Brave-Browser\\User Data\\Default",
    other: "macOS : ~/Library/Application Support/BraveSoftware/Brave-Browser/Default · Linux : ~/.config/BraveSoftware/Brave-Browser/Default",
  },
  {
    b: "Opera",
    win: "%APPDATA%\\Opera Software\\Opera Stable",
    other: "macOS : ~/Library/Application Support/com.operasoftware.Opera · Linux : ~/.config/opera",
  },
  {
    b: "Opera GX",
    win: "%APPDATA%\\Opera Software\\Opera GX Stable",
    other: "macOS : ~/Library/Application Support/com.operasoftware.OperaGX",
  },
  {
    b: "Vivaldi",
    win: "%LOCALAPPDATA%\\Vivaldi\\User Data\\Default",
    other: "macOS : ~/Library/Application Support/Vivaldi/Default · Linux : ~/.config/vivaldi/Default",
  },
  {
    b: "Yandex",
    win: "%LOCALAPPDATA%\\Yandex\\YandexBrowser\\User Data\\Default",
    other: "macOS : ~/Library/Application Support/Yandex/YandexBrowser/Default",
  },
  {
    b: "Chromium",
    win: "%LOCALAPPDATA%\\Chromium\\User Data\\Default",
    other: "Linux : ~/.config/chromium/Default",
  },
  {
    b: "Arc",
    win: "%LOCALAPPDATA%\\Packages\\TheBrowserCompany.Arc_*\\LocalCache\\…\\User Data\\Default",
    other: "macOS : ~/Library/Application Support/Arc/User Data/Default",
  },
  {
    b: "Firefox",
    win: "%APPDATA%\\Mozilla\\Firefox\\Profiles\\<id>.default-release",
    other: "macOS : ~/Library/Application Support/Firefox/Profiles · Linux : ~/.mozilla/firefox",
  },
  {
    b: "Safari",
    win: "— (macOS uniquement)",
    other: "macOS : ~/Library/Safari (History.db, Bookmarks.plist, Downloads.plist)",
  },
  {
    b: "Samsung Internet",
    win: "— (mobile uniquement — voir tableau Mobile)",
    other: "—",
  },
  {
    b: "UC Browser",
    win: "%APPDATA%\\UCBrowser (ancien, surtout mobile)",
    other: "—",
  },
  {
    b: "IE / Edge Legacy",
    win: "%LOCALAPPDATA%\\Microsoft\\Windows\\WebCache\\WebCacheV01.dat",
    other: "—",
  },
];

// --- Référentiel mobile. iOS 14→27 : conteneurs d'app (GUID aléatoire) sous
//     /private/var/mobile/Containers/Data/Application/<GUID>/ (extraction de
//     système de fichiers complète : Cellebrite FFS, GrayKey, checkm8…).
//     Android 12→16 : /data/data/<paquet>/ (extraction physique / FFS). ---
const HELP_MOBILE: { b: string; ios: string; android: string }[] = [
  {
    b: "Safari",
    ios: "/private/var/mobile/Library/Safari/History.db · …/Bookmarks.db · SuspendState",
    android: "— (iOS uniquement)",
  },
  {
    b: "Chrome",
    ios: "com.google.chrome.ios → <GUID>/Library/Application Support/Google/Chrome/Default/History",
    android: "com.android.chrome → app_chrome/Default/History, Cookies, Login Data",
  },
  {
    b: "Edge",
    ios: "com.microsoft.msedge → <GUID>/Library/Application Support/.../Default/History",
    android: "com.microsoft.emmx → app_chrome/Default/History",
  },
  {
    b: "Firefox",
    ios: "org.mozilla.ios.Firefox → <GUID>/…/profile.profile/places.sqlite (ou browser.db)",
    android: "org.mozilla.firefox → files/<profil>/places.sqlite, cookies.sqlite",
  },
  {
    b: "Samsung Internet",
    ios: "— (Android uniquement)",
    android: "com.sec.android.app.sbrowser → app_sbrowser/Default/History",
  },
  {
    b: "Opera / Opera GX",
    ios: "com.opera.OperaTouch · com.opera.mini.ios → <GUID>/…/History",
    android: "com.opera.browser · com.opera.gx → app_opera/.../History",
  },
  {
    b: "Opera Mini",
    ios: "com.opera.mini.ios",
    android: "com.opera.mini.native → files/ (format propriétaire)",
  },
  {
    b: "UC Browser",
    ios: "com.ucweb.iosbrowser",
    android: "com.UCMobile.intl / com.ucmobile → …/UCDownloads, bases SQLite d'historique",
  },
  {
    b: "Brave",
    ios: "com.brave.ios.browser → <GUID>/…/Default/History",
    android: "com.brave.browser → app_chrome/Default/History",
  },
  {
    b: "Vivaldi",
    ios: "— (surtout Android)",
    android: "com.vivaldi.browser → app_vivaldi/Default/History",
  },
  {
    b: "Yandex",
    ios: "ru.yandex.mobile.search",
    android: "com.yandex.browser → app_yandex/Default/History",
  },
  {
    b: "Arc",
    ios: "company.thebrowser.arc → <GUID>/…/User Data/Default/History",
    android: "— (pas d'application Android)",
  },
];

// fichiers clés à fournir par navigateur
const HELP_FILES =
  "Chromium (Chrome/Edge/Brave/Opera/Vivaldi/Yandex/Arc/Samsung/UC) : History, Cookies (ou Network\\Cookies), Web Data, Login Data, Bookmarks, Shortcuts, Top Sites, Favicons, Preferences. " +
  "Firefox (bureau & Fenix Android) : places.sqlite, cookies.sqlite, formhistory.sqlite, logins.json, favicons.sqlite, sessionstore.jsonlz4. " +
  "Safari (macOS & iOS) : History.db, Bookmarks.plist/db, Downloads.plist. IE/Edge Legacy : WebCacheV01.dat. " +
  "L'auto-détection repose sur le CONTENU (signatures + tables SQLite) : elle reconnaît ces bases quel que soit le chemin, y compris depuis une extraction de système de fichiers mobile montée en dossier.";

// guide d'utilisation affiché en tête du panneau d'aide
const HELP_GUIDE: { t: string; items: string[] }[] = [
  {
    t: "1. Charger les artefacts",
    items: [
      "Auto-détection : profils des navigateurs installés sur ce poste (fermez-les d'abord).",
      "Dossier : scan récursif d'un dossier de profils extraits (X-Ways, Cellebrite…).",
      "Fichiers : une ou plusieurs bases précises.",
      "Image E01 ou brute : lecture des volumes NTFS (Windows) et ext4 (Android). APFS (iOS, macOS) et F2FS (Android récent) ne sont pas lus.",
    ],
  },
  {
    t: "2. Naviguer",
    items: [
      "Colonne gauche : un bloc par navigateur, avec ses sous-onglets (historique, favoris, cookies, téléchargements…). Chaque entrée affiche son nombre entre parenthèses.",
      "Colonne Résultats : éléments d'identité (par catégories dépliantes : paiement, téléphones, emails, adresses, identité civile, sécurité sociale, véhicules), téléchargements, géolocalisations, VPN, éléments récupérés, crypto-actifs, synchronisation, extensions, IOC, mots de passe Firefox, recherches / mots-clés, éléments marqués.",
      "Les colonnes d'affichage s'adaptent automatiquement au contenu.",
    ],
  },
  {
    t: "3. Chronologie",
    items: [
      "Frise sous chaque vue datée : sélectionnez une période pour filtrer le tableau.",
      "Clic droit maintenu et déplacement : déplacement dans la frise.",
      "Survol : infobulle avec la date et le nombre d'éléments.",
      "Au zoom sur 3 jours ou moins, la frise passe en barres verticales.",
      "Dates affichées au format jj-mm-aaaa.",
    ],
  },
  {
    t: "4. Marquer et rechercher",
    items: [
      "Marquer : attribuez un marqueur, une couleur et une note à une ligne. Les éléments marqués sont regroupés dans Résultats › Éléments marqués.",
      "Recherche globale : cherche un terme dans tous les artefacts chargés.",
      "Résultats › Recherches / mots-clés : n'affiche que les éléments marqués issus des recherches, avec le nombre de tagués et de trouvés.",
    ],
  },
  {
    t: "5. Carte et fuseaux",
    items: [
      "Géolocalisations : carte Leaflet, liens OpenStreetMap cliquables.",
      "Heatmap : répartition horaire selon le fuseau choisi (24 h).",
    ],
  },
  {
    t: "6. Rapport et exports",
    items: [
      "Rapport HTML ou PDF horodaté, avec les éléments marqués et les recherches par mots-clés.",
      "Chaque export porte son empreinte SHA-256. Les vues « Éléments marqués » et « Recherches / mots-clés » s'exportent via le rapport.",
    ],
  },
  {
    t: "7. Fonctions particulières",
    items: [
      "Mots de passe Firefox : déchiffrement de key4.db et logins.json, sur copie uniquement. Mot de passe maître optionnel. Un champ affiché « <erreur : …> » indique un échec.",
      "Mots de passe Chromium : liste des identifiants (URL, identifiant, état du mot de passe) par navigateur. Les mots de passe ne sont pas déchiffrés : Chrome et Edge récents utilisent un chiffrement lié au service du navigateur.",
      "Carving SQLite : récupération de lignes supprimées dans les bases chargées.",
      "IOC : chargement d'une liste d'indicateurs à croiser avec les artefacts.",
    ],
  },
  {
    t: "8. Principes",
    items: [
      "Lecture seule : les sources ne sont jamais modifiées. Les fichiers -wal et -shm sont copiés avec leur base.",
      "Empreintes SHA-256 pour les sources et les exports. Horodatages convertis depuis le référentiel de chaque navigateur.",
    ],
  },
];

// signature stable d'une ligne (independante du fuseau), pour le marquage
function rowSig(r: Row, columns: string[]): string {
  return columns
    .map((c) => {
      const cell = r.values[c];
      if (!cell || cell.t === "null") return "";
      return `${cell.t}:${"v" in cell ? cell.v : ""}`;
    })
    .join("␟");
}
// colonnes et lignes d'une liste de marquages (vues « Éléments marqués » et
// « Recherches / mots-clés » restreintes aux éléments marqués)
const TAG_COLUMNS = [
  "Marqueur",
  "Navigateur",
  "Source (chemin)",
  "Date fichier",
  "Taille",
  "Élément",
  "Note",
  "Marqué le",
];
// origines de marquage rattachées à la vue Résultats (recherches)
const KW_ORIGINS = ["kw", "search"];
// catégories métier des éléments d'identité (types produits par le moteur) ;
// tout type non listé tombe dans « Autres »
const IDENT_GROUPS: { key: string; label: string; icon: string; types: string[] }[] = [
  { key: "paiement", label: "Moyens de paiement", icon: "💳", types: ["Carte bancaire", "IBAN / banque", "Banque"] },
  { key: "telephone", label: "Téléphones", icon: "📞", types: ["Telephone"] },
  { key: "email", label: "Emails", icon: "✉️", types: ["Email"] },
  { key: "adresse", label: "Adresses", icon: "🏠", types: ["Adresse"] },
  { key: "civil", label: "Identité civile", icon: "🪪", types: ["Nom / Prenom", "Date de naissance"] },
  { key: "secu", label: "Sécurité sociale", icon: "🛡️", types: ["Securite sociale (NIR)", "Securite sociale"] },
  { key: "vehicule", label: "Véhicules", icon: "🚗", types: ["Immatriculation", "Vehicule / immatriculation"] },
];
const IDENT_OTHER = { key: "autres", label: "Autres", icon: "📁" };
function identGroupOf(type: string): string {
  const g = IDENT_GROUPS.find((x) => x.types.includes(type));
  return g ? g.key : IDENT_OTHER.key;
}
// origine d'après la vue où l'on marque (recherche globale, mots-clés, Résultats)
function viewOrigin(kind: string | undefined): string {
  if (kind === "keywords" || kind === "kwresults") return "kw";
  if (kind === "search") return "search";
  return "";
}
// origine d'un marquage : celle enregistrée, sinon déduite du titre de la vue
// d'origine (marquages faits avant l'enregistrement de l'origine)
function tagOrigin(t: TagEntry): string {
  if (t.origin) return t.origin;
  const c = t.context ?? "";
  if (c.startsWith("Recherche :")) return "search";
  if (c.startsWith("Recherche par mots-clés") || c.startsWith("Recherches / mots-clés")) return "kw";
  return "";
}
function tagRows(list: TagEntry[]): { columns: string[]; rows: Row[] } {
  const rows: Row[] = list.map((t) => ({
    values: {
      Marqueur: { t: "text", v: t.label },
      Navigateur: { t: "text", v: t.browser },
      "Source (chemin)": { t: "text", v: t.path },
      "Date fichier": t.modified != null ? { t: "time", v: t.modified } : { t: "null" },
      Taille: t.size != null ? { t: "int", v: t.size } : { t: "null" },
      Élément: { t: "text", v: t.summary },
      Note: { t: "text", v: t.note },
      "Marqué le": { t: "text", v: t.when },
      _sig: { t: "text", v: t.sig },
      _couleur: { t: "text", v: t.color },
    },
    raw: [],
  }));
  return { columns: TAG_COLUMNS, rows };
}
// nombre de correspondances réelles (hors ligne « aucun résultat »)
function countHits(rows: Row[]): number {
  return rows.filter((r) => cellText(r.values["Valeur"]) !== "(aucun résultat)").length;
}

function rowSummary(r: Row, columns: string[], tz: string): string {
  // Conserve l'intégralité des colonnes visibles de la ligne (et non les 3
  // premières seulement) afin qu'un marquage retienne tout le résultat —
  // notamment la valeur et le terme trouvé lors d'une recherche par mots-clés.
  return columns
    .filter((c) => !c.startsWith("_"))
    .map((c) => {
      const v = fmtCell(r.values[c], tz);
      return v ? `${c}: ${v}` : "";
    })
    .filter(Boolean)
    .join(" — ")
    .slice(0, 600);
}
function cellText(c: Row["values"][string] | undefined): string {
  return c && c.t === "text" ? c.v : "";
}
// pastille navigateur (couleur de marque + initiale) — pas de logo (droits)
function browserBadge(name: string): { bg: string; ch: string } {
  const n = name.toLowerCase();
  if (n.includes("chrome")) return { bg: "#4285F4", ch: "C" };
  if (n.includes("edge legacy") || n.includes("internet explorer"))
    return { bg: "#1EBBEE", ch: "e" };
  if (n.includes("edge")) return { bg: "#0E7FC1", ch: "E" };
  if (n.includes("firefox")) return { bg: "#FF7139", ch: "F" };
  if (n.includes("brave")) return { bg: "#FB542B", ch: "B" };
  if (n.includes("opera")) return { bg: "#FF1B2D", ch: "O" };
  if (n.includes("vivaldi")) return { bg: "#EF3939", ch: "V" };
  if (n.includes("safari")) return { bg: "#1B88CA", ch: "S" };
  if (n.includes("chromium")) return { bg: "#6E7D8A", ch: "c" };
  return { bg: "#64748b", ch: "?" };
}
// identité d'une ligne pour le marquage : signature réelle portée par la ligne
// (vue « Éléments marqués ») sinon signature calculée depuis ses valeurs
function sigOf(r: Row, columns: string[]): string {
  const s = r.values["_sig"];
  if (s && s.t === "text") return s.v;
  return rowSig(r, columns);
}
// millisecondes d'une ligne = premiere cellule de type temps
function rowTimeMs(r: Row): number | null {
  for (const k in r.values) {
    const cell = r.values[k];
    if (cell && cell.t === "time") return cell.v;
  }
  return null;
}

export function App() {
  const [over, setOver] = useState<Overview[]>([]);
  const [groups, setGroups] = useState<BrowserGroup[]>([]);
  // Éléments d'identité : bloc et catégories fermés par défaut.
  const [collapsed, setCollapsed] = useState<Set<string>>(
    () => new Set(["identity", ...IDENT_GROUPS.map((g) => "identity:" + g.key), "passwords"])
  );
  const [sel, setSel] = useState<Selection | null>(null);
  const [view, setView] = useState<View | null>(null);
  const [theme, setTheme] = useState<string>("clair");
  const [tz, setTz] = useState<string>("Local");
  const [noisy, setNoisy] = useState<boolean>(false);
  const [filter, setFilter] = useState<string>("");
  const [dateFrom, setDateFrom] = useState<string>("");
  const [dateTo, setDateTo] = useState<string>("");
  // Timeline interactive (vue Chronologie) : fenêtre de zoom + sélection.
  const [timeWin, setTimeWin] = useState<[number, number] | null>(null);
  const [timeSel, setTimeSel] = useState<[number, number] | null>(null);
  const [tags, setTags] = useState<TagEntry[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; row: Row } | null>(null);
  const [tagModalRows, setTagModalRows] = useState<Row[] | null>(null);
  const [kwLists, setKwLists] = useState<KwList[]>(loadKwLists);
  const [kwQuery, setKwQuery] = useState<KwItem[]>([]);
  const [showKw, setShowKw] = useState<boolean>(false);
  const [kwSel, setKwSel] = useState<number>(0);
  const [showReportOpts, setShowReportOpts] = useState<boolean>(false);
  const [reportOpts, setReportOpts] = useState<ReportOptions>(DEFAULT_REPORT_OPTS);
  // Éléments d'identité retenus (sélectionnés) pour insertion dans le rapport.
  const [identityPick, setIdentityPick] = useState<Record<string, string>[]>([]);
  // Menu déroulant « Export » (CSV / XLSX / Rapport).
  const [showExport, setShowExport] = useState<boolean>(false);
  const [media, setMedia] = useState<MediaItem[]>([]);
  const [zoomMedia, setZoomMedia] = useState<MediaItem | null>(null);
  const [htmls, setHtmls] = useState<HtmlItem[]>([]);
  const [htmlView, setHtmlView] = useState<HtmlItem | null>(null);
  const [gsearch, setGsearch] = useState<string>("");
  // Compteurs « trouvés » de la vue Résultats : correspondances de mots-clés
  // (session) et de recherches globales (par terme, session).
  const [kwFound, setKwFound] = useState<number>(0);
  const [searchFound, setSearchFound] = useState<Record<string, number>>({});
  // Nombre d'éléments de chaque entrée de la colonne Résultats (moteur).
  const [resCounts, setResCounts] = useState<Record<string, number>>({});
  // Nombre d'éléments d'identité par type (sous-catégories de la colonne Résultats).
  const [identCounts, setIdentCounts] = useState<Record<string, number>>({});
  // Déchiffrement des mots de passe Firefox (NSS).
  const [fxPwd, setFxPwd] = useState<string>("");
  const [fxDir, setFxDir] = useState<string | null>(null);
  const [showFxPrompt, setShowFxPrompt] = useState<boolean>(false);
  const [fxPromptVal, setFxPromptVal] = useState<string>("");
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortAsc, setSortAsc] = useState<boolean>(true);
  const [detail, setDetail] = useState<Row | null>(null);
  const [tagLabel, setTagLabel] = useState<string>(TAG_PRESETS[0].name);
  const [tagColor, setTagColor] = useState<string>(TAG_PRESETS[0].color);
  const [tagNote, setTagNote] = useState<string>("");
  const [showCase, setShowCase] = useState<boolean>(false);
  const [showHelp, setShowHelp] = useState<boolean>(false);
  const [showSources, setShowSources] = useState<boolean>(false);
  const [browserDraft, setBrowserDraft] = useState<Record<number, string>>({});
  const [appVersion, setAppVersion] = useState<string>("");
  const [caseInfo, setCaseInfo] = useState<CaseInfo>({
    numero: "",
    examinateur: "",
    service: "",
    notes: "",
  });

  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
  }, [theme]);

  const refreshResCounts = useCallback(() => {
    api.resultsCounts().then(setResCounts).catch(() => {});
    api
      .identity()
      .then((v) => {
        const c: Record<string, number> = {};
        for (const r of v.rows) {
          const t = cellText(r.values["Type"]) || "Autre";
          c[t] = (c[t] ?? 0) + 1;
        }
        setIdentCounts(c);
      })
      .catch(() => {});
  }, []);

  const refreshOverview = useCallback(async () => {
    try {
      const [ov, gr] = await Promise.all([api.overview(), api.browserGroups()]);
      setOver(ov);
      setGroups(gr);
      refreshResCounts();
    } catch (e) {
      const txt = e instanceof Error ? e.message : String(e);
      await message(txt, { title: "NavForensics — erreur", kind: "error" });
    }
  }, []);

  useEffect(() => {
    refreshOverview();
    api.listTags().then(setTags).catch(() => {});
    api.version().then(setAppVersion).catch(() => {});
    api.kwResults().then((v) => setKwFound(countHits(v.rows))).catch(() => {});
  }, [refreshOverview]);

  // index des signatures marquees + acces par signature
  const tagBySig = useMemo(() => {
    const m = new Map<string, TagEntry>();
    for (const t of tags) m.set(t.sig, t);
    return m;
  }, [tags]);

  // persistance des listes de mots-cles
  useEffect(() => {
    try {
      localStorage.setItem("nf_kwlists", JSON.stringify(kwLists));
    } catch {
      /* ignore */
    }
  }, [kwLists]);

  // reinitialise la selection et ferme le menu contextuel en changeant de vue
  useEffect(() => {
    setSelected(new Set());
    setCtxMenu(null);
  }, [sel]);

  const loadView = useCallback(async () => {
    if (!sel) {
      setView(null);
      return;
    }
    setSortCol(null);
    try {
      if (sel.kind === "cat") {
        const c = await api.category(sel.ai, sel.ci);
        if (c) {
          setView({
            title: `${over[sel.ai]?.evidence ?? ""} — ${c.name}`,
            sha: over[sel.ai]?.sha256 ?? "",
            columns: c.columns,
            rows: c.rows,
          });
        }
      } else if (sel.kind === "timeline") {
        const v = await api.timeline(noisy);
        setView({ title: "Timeline unifiée", sha: "— (fusion)", ...v });
      } else if (sel.kind === "domains") {
        const v = await api.domains(noisy);
        setView({ title: "Regroupement par domaine", sha: "—", ...v });
      } else if (sel.kind === "search") {
        const v = await api.search(sel.term);
        setSearchFound((prev) => ({ ...prev, [sel.term]: v.rows.length }));
        setView({ title: `Recherche : « ${sel.term} »`, sha: "—", ...v });
      } else if (sel.kind === "flagged") {
        const v = await api.flagged(sel.browser);
        setView({
          title: sel.browser
            ? `Liens signalés — ${sel.browser}`
            : "Liens signalés — tous navigateurs",
          sha: "—",
          ...v,
        });
      } else if (sel.kind === "search_terms") {
        const v = await api.searchTerms();
        setView({ title: "Termes recherchés", sha: "—", ...v });
      } else if (sel.kind === "stats") {
        const v = await api.stats();
        setView({ title: "Statistiques", sha: "—", ...v });
      } else if (sel.kind === "ga") {
        const v = await api.gaCookies();
        setView({ title: "Cookies Google Analytics", sha: "—", ...v });
      } else if (sel.kind === "histogram") {
        const v = await api.histogram();
        setView({ title: "Histogramme d'activité (UTC)", sha: "—", ...v });
      } else if (sel.kind === "media" || sel.kind === "html") {
        setView(null); // galerie / pages HTML rendues séparément
      } else if (sel.kind === "online") {
        const v = await api.onlineServices();
        setView({ title: "Services en ligne (cloud / webmail / réseaux)", sha: "—", ...v });
      } else if (sel.kind === "identity") {
        const v = await api.identity();
        let rows = v.rows;
        let title = "Éléments d'identité (autofill)";
        if (sel.type) {
          const t = sel.type;
          rows = rows.filter((r) => cellText(r.values["Type"]) === t);
          title = `Éléments d'identité — ${t}`;
        } else if (sel.group) {
          const g = sel.group;
          rows = rows.filter((r) => identGroupOf(cellText(r.values["Type"])) === g);
          const lbl = IDENT_GROUPS.find((x) => x.key === g)?.label ?? IDENT_OTHER.label;
          title = `Éléments d'identité — ${lbl}`;
        }
        setView({ title, sha: "—", columns: v.columns, rows });
      } else if (sel.kind === "downloads") {
        const v = await api.downloads();
        setView({ title: "Téléchargements", sha: "—", ...v });
      } else if (sel.kind === "geolocation") {
        const v = await api.geolocation();
        setView({ title: "Géolocalisations / adresses recherchées", sha: "—", ...v });
      } else if (sel.kind === "vpn") {
        const v = await api.vpn();
        setView({ title: "VPN / anonymiseurs", sha: "—", ...v });
      } else if (sel.kind === "recovered") {
        const v = await api.recovered();
        setView({ title: "Enregistrements supprimés (récupérés)", sha: "—", ...v });
      } else if (sel.kind === "crypto") {
        const v = await api.crypto();
        setView({ title: "Crypto-actifs", sha: "—", ...v });
      } else if (sel.kind === "kwresults") {
        // Résultats : uniquement les éléments marqués issus de la recherche par
        // mots-clés ou de la recherche globale.
        const picked = tags.filter((t) => KW_ORIGINS.includes(tagOrigin(t)));
        const { columns, rows } = tagRows(picked);
        setView({
          title: `Recherches / mots-clés — éléments marqués (${picked.length})`,
          sha: "—",
          columns,
          rows,
        });
      } else if (sel.kind === "fxpasswords") {
        const v = await api.firefoxPasswords(fxPwd, fxDir);
        refreshResCounts();
        setView({ title: "Mots de passe Firefox (déchiffrés)", sha: "—", ...v });
      } else if (sel.kind === "chrpasswords") {
        const v = await api.chromiumPasswords();
        const rows = v.rows.filter((r) => cellText(r.values["Navigateur"]) === sel.browser);
        refreshResCounts();
        setView({
          title: `Mots de passe ${sel.browser} (${rows.length}) — non déchiffrés`,
          sha: "—",
          columns: v.columns,
          rows,
        });
      } else if (sel.kind === "ioc") {
        const v = await api.iocResults();
        setView({ title: "Correspondances IOC", sha: "—", ...v });
      } else if (sel.kind === "sync") {
        const v = await api.syncAccounts();
        setView({ title: "Comptes synchronisés", sha: "—", ...v });
      } else if (sel.kind === "extensions") {
        const v = await api.extensions();
        setView({ title: "Extensions installées", sha: "—", ...v });
      } else if (sel.kind === "geopoints") {
        const v = await api.geopoints();
        setView({ title: "Points géolocalisés", sha: "—", ...v });
      } else if (sel.kind === "heatmap") {
        const v = await api.heatmap(tz);
        const zone = tz === "Local" ? "heure locale" : "UTC";
        setView({ title: `Pattern of life (jour × heure, ${zone})`, sha: "—", ...v });
      } else if (sel.kind === "keywords") {
        const v = await api.keywordSearch(kwQuery);
        // met à jour le compteur « trouvés » de la vue Résultats
        const acc = await api.kwResults().catch(() => null);
        if (acc) setKwFound(countHits(acc.rows));
        setView({
          title: `Recherche par mots-clés (${kwQuery.length} termes)`,
          sha: "—",
          ...v,
        });
      } else if (sel.kind === "tags") {
        const { columns, rows } = tagRows(tags);
        setView({ title: `Éléments marqués (${tags.length})`, sha: "—", columns, rows });
      }
    } catch (e) {
      const txt = e instanceof Error ? e.message : String(e);
      await message(txt, { title: "NavForensics — erreur", kind: "error" });
    }
  }, [sel, noisy, over, tags, kwQuery, tz, fxPwd, fxDir]);

  useEffect(() => {
    loadView();
  }, [loadView]);

  // pre-remplit le marqueur/la note a l'ouverture du detail d'une ligne
  useEffect(() => {
    if (detail && view) {
      const ex = tagBySig.get(sigOf(detail, view.columns));
      setTagLabel(ex?.label ?? TAG_PRESETS[0].name);
      setTagColor(ex?.color ?? TAG_PRESETS[0].color);
      setTagNote(ex?.note ?? "");
    }
  }, [detail, view, tagBySig]);

  // affiche toute erreur backend au lieu de laisser la promesse echouer en silence
  const reportError = async (e: unknown) => {
    const txt = e instanceof Error ? e.message : String(e);
    await message(txt, { title: "NavForensics — erreur", kind: "error" });
  };

  const summary = async (s: { loaded: number; errors: string[] }) => {
    let msg = `${s.loaded} base(s) chargée(s).`;
    if (s.errors.length) {
      msg += `\n\n${s.errors.length} ignorée(s) :\n` + s.errors.slice(0, 12).join("\n");
      await message(msg, { title: "NavForensics", kind: "warning" });
    }
  };

  const addFiles = async () => {
    try {
      const picked = await open({ multiple: true, title: "Bases de navigateur" });
      if (!picked) return;
      const paths = Array.isArray(picked) ? picked : [picked];
      await summary(await api.addPaths(paths));
      await refreshOverview();
    } catch (e) {
      await reportError(e);
    }
  };
  const loadE01 = async () => {
    try {
      const picked = await open({
        multiple: false,
        title: "Image forensique (E01 / brute)",
        filters: [
          { name: "Images E01 / brutes", extensions: ["E01", "Ex01", "L01", "001", "raw", "dd", "img"] },
          { name: "Tous les fichiers", extensions: ["*"] },
        ],
      });
      if (!picked) return;
      const path = Array.isArray(picked) ? picked[0] : picked;
      await summary(await api.loadE01(path));
      await refreshOverview();
    } catch (e) {
      await reportError(e);
    }
  };
  const loadIoc = async () => {
    try {
      const picked = await open({
        multiple: false,
        title: "Liste de surveillance (IOC) — un indicateur par ligne",
      });
      if (!picked) return;
      const path = Array.isArray(picked) ? picked[0] : picked;
      const v = await api.iocLoad(path);
      refreshResCounts();
      setSel({ kind: "ioc" });
      await message(`${v.rows.length} correspondance(s) trouvée(s).`, {
        title: "NavForensics",
      });
    } catch (e) {
      await reportError(e);
    }
  };
  const addFolder = async () => {
    try {
      const dir = await open({ directory: true, title: "Dossier à scanner" });
      if (!dir || Array.isArray(dir)) return;
      await summary(await api.addFolder(dir));
      await refreshOverview();
    } catch (e) {
      await reportError(e);
    }
  };
  const addCache = async () => {
    try {
      const dir = await open({ directory: true, title: "Dossier de cache (carving)" });
      if (!dir || Array.isArray(dir)) return;
      await summary(await api.addCache(dir));
      await refreshOverview();
    } catch (e) {
      await reportError(e);
    }
  };
  const autodetectRun = async () => {
    try {
      await summary(await api.autodetect());
      await refreshOverview();
    } catch (e) {
      await reportError(e);
    }
  };
  const applyBrowser = async (ai: number) => {
    const val = (browserDraft[ai] ?? over[ai]?.browser ?? "").trim();
    if (!val) return;
    try {
      await api.setBrowser(ai, val);
      await refreshOverview();
    } catch (e) {
      await reportError(e);
    }
  };
  const loadMedia = async () => {
    try {
      const dir = await open({
        directory: true,
        title: "Dossier de cache / profil (images & vidéos)",
      });
      if (!dir || Array.isArray(dir)) return;
      const [items, pages] = await Promise.all([api.cacheMedia(dir), api.cacheHtml(dir)]);
      setMedia(items);
      setHtmls(pages);
      setSel({ kind: items.length > 0 ? "media" : "html" });
      if (items.length === 0 && pages.length === 0)
        await message("Aucune image, vidéo ou page HTML détectée dans ce dossier.", {
          title: "NavForensics",
        });
    } catch (e) {
      await reportError(e);
    }
  };
  const saveMediaItem = async (m: MediaItem) => {
    try {
      const ext = m.mime.split("/")[1] || "bin";
      const path = await save({
        defaultPath: m.name + "." + ext,
        filters: [{ name: m.mime, extensions: [ext] }],
      });
      if (!path) return;
      await api.saveMedia(path, m.data);
      await message("Média exporté : " + path, { title: "NavForensics" });
    } catch (e) {
      await reportError(e);
    }
  };
  const clearAll = async () => {
    try {
      const ok = await ask("Vider tous les éléments chargés ?", {
        title: "Confirmation",
        kind: "warning",
      });
      if (!ok) return;
      await api.clear();
      setSel(null);
      setView(null);
      await refreshOverview();
    } catch (e) {
      await reportError(e);
    }
  };

  const runSearch = () => {
    const t = gsearch.trim();
    if (t) setSel({ kind: "search", term: t });
  };

  const openItem = (it: SubItem) => {
    if (it.kind === "cat" && it.ai !== null && it.ci !== null) {
      setSel({ kind: "cat", ai: it.ai, ci: it.ci });
    } else if (it.kind === "flagged") {
      setSel({ kind: "flagged", browser: it.browser });
    }
  };

  // filtre + tri cote client
  const displayed = useMemo<Row[]>(() => {
    if (!view) return [];
    let rows = view.rows;
    const f = filter.trim().toLowerCase();
    if (f) {
      rows = rows.filter((r) =>
        view.columns.some((c) => fmtCell(r.values[c], tz).toLowerCase().includes(f))
      );
    }
    // filtre par plage de dates (UTC) ; les lignes sans date sont conservees
    const fromMs = dateFrom ? Date.parse(dateFrom + "T00:00:00Z") : null;
    const toMs = dateTo ? Date.parse(dateTo + "T23:59:59.999Z") : null;
    if (fromMs !== null || toMs !== null) {
      rows = rows.filter((r) => {
        const ms = rowTimeMs(r);
        if (ms === null) return true;
        if (fromMs !== null && ms < fromMs) return false;
        if (toMs !== null && ms > toMs) return false;
        return true;
      });
    }
    // sélection de la timeline interactive (toutes vues datées) ; timeSel est
    // remis à zéro à chaque changement de vue, donc filtrer dès qu'il est posé.
    if (timeSel) {
      const [a, b] = timeSel;
      rows = rows.filter((r) => {
        const ms = rowTimeMs(r);
        return ms !== null && ms >= a && ms <= b;
      });
    }
    if (sortCol) {
      rows = [...rows].sort((a, b) => {
        const ka = cellSortKey(a.values[sortCol]);
        const kb = cellSortKey(b.values[sortCol]);
        let r = 0;
        if (ka < kb) r = -1;
        else if (ka > kb) r = 1;
        return sortAsc ? r : -r;
      });
    }
    return rows;
  }, [view, filter, tz, sortCol, sortAsc, dateFrom, dateTo, sel, timeSel]);

  // largeurs « naturelles » (selon le nom de colonne)
  const defaultWidths = useMemo<number[]>(() => {
    if (!view) return [];
    return view.columns.map((c) => Math.min(Math.max(c.length * 9, 110), 420));
  }, [view]);

  // largeur disponible du tableau (suivie en temps réel pour l'auto-ajustement)
  const [availW, setAvailW] = useState(0);
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setAvailW(el.clientWidth));
    ro.observe(el);
    setAvailW(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  // largeurs de base : étirées pour remplir la fenêtre quand elles tiennent,
  // sinon largeurs naturelles (défilement horizontal). S'applique à tous les onglets.
  const baseWidths = useMemo<number[]>(() => {
    if (defaultWidths.length === 0) return defaultWidths;
    const natural = defaultWidths.reduce((a, b) => a + b, 0);
    const target = availW > 2 ? availW - 2 : natural;
    if (natural <= 0 || target <= natural) return defaultWidths;
    const k = target / natural;
    const arr = defaultWidths.map((w) => Math.floor(w * k));
    const diff = target - arr.reduce((a, b) => a + b, 0);
    if (arr.length) arr[arr.length - 1] += diff;
    return arr;
  }, [defaultWidths, availW]);

  // largeurs effectives, redimensionnables par l'utilisateur
  const [colW, setColW] = useState<number[]>([]);
  useEffect(() => {
    setColW(baseWidths);
  }, [baseWidths]);
  const widths = colW.length === baseWidths.length ? colW : baseWidths;
  const totalWidth = useMemo(() => widths.reduce((a, b) => a + b, 0), [widths]);

  // Évènements datés de la vue courante (pour la timeline interactive, sur
  // toutes les vues qui portent des dates : chronologie, historiques, visites,
  // termes recherchés, téléchargements, etc.).
  const allEvents = useMemo<TLEvent[]>(() => {
    if (!view) return [];
    const out: TLEvent[] = [];
    for (const r of view.rows) {
      const ms = rowTimeMs(r);
      if (ms == null) continue;
      const ty = r.values["Type"];
      out.push({ t: ms, type: ty && ty.t === "text" ? ty.v : "Consultations" });
    }
    return out;
  }, [view]);
  const timeExtent = useMemo<[number, number] | null>(() => {
    if (allEvents.length === 0) return null;
    let lo = allEvents[0].t;
    let hi = allEvents[0].t;
    for (const e of allEvents) {
      if (e.t < lo) lo = e.t;
      if (e.t > hi) hi = e.t;
    }
    if (hi <= lo) hi = lo + 86400000;
    return [lo, hi];
  }, [allEvents]);
  // Vues sans timeline (visualisation propre ou sans date par ligne).
  const TL_EXCLUDE = ["heatmap", "geopoints", "media", "domains", "stats", "histogram", "sync", "extensions"];
  const showTimeline = !!timeExtent && !TL_EXCLUDE.includes(sel?.kind ?? "");
  // Réinitialise zoom et sélection en changeant de vue.
  useEffect(() => {
    setTimeWin(null);
    setTimeSel(null);
  }, [sel]);

  // Points pour la carte Leaflet (vue « Points géolocalisés »).
  const geoPoints = useMemo(() => {
    if (sel?.kind !== "geopoints" || !view) return [];
    return displayed
      .map((r) => {
        const lat = parseFloat(cellText(r.values["Latitude"]));
        const lon = parseFloat(cellText(r.values["Longitude"]));
        const label =
          cellText(r.values["Lieu"]) || cellText(r.values["Service"]) || "";
        const url = cellText(r.values["Carte (OSM)"]) || cellText(r.values["URL"]);
        return { lat, lon, label, url };
      })
      .filter((p) => Number.isFinite(p.lat) && Number.isFinite(p.lon));
  }, [sel, view, displayed]);

  // glisser le bord droit d'un en-tete pour redimensionner la colonne
  const resizing = useRef<{ idx: number; startX: number; startW: number } | null>(null);
  const onResizeMove = useCallback((e: MouseEvent) => {
    const r = resizing.current;
    if (!r) return;
    const w = Math.max(50, r.startW + (e.clientX - r.startX));
    setColW((prev) => {
      const base = prev.slice();
      base[r.idx] = w;
      return base;
    });
  }, []);
  const onResizeEnd = useCallback(() => {
    resizing.current = null;
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
    window.removeEventListener("mousemove", onResizeMove);
    window.removeEventListener("mouseup", onResizeEnd);
  }, [onResizeMove]);
  const onResizeStart = useCallback(
    (e: React.MouseEvent, idx: number) => {
      e.preventDefault();
      e.stopPropagation();
      resizing.current = { idx, startX: e.clientX, startW: widths[idx] ?? 120 };
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
      window.addEventListener("mousemove", onResizeMove);
      window.addEventListener("mouseup", onResizeEnd);
    },
    [widths, onResizeMove, onResizeEnd]
  );
  // double-clic sur la poignee : revenir a la largeur par defaut
  const onResizeReset = useCallback(
    (e: React.MouseEvent, idx: number) => {
      e.preventDefault();
      e.stopPropagation();
      setColW((prev) => {
        const base = prev.length ? prev.slice() : baseWidths.slice();
        base[idx] = baseWidths[idx];
        return base;
      });
    },
    [baseWidths]
  );

  const rowVirt = useVirtualizer({
    count: displayed.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 24,
    overscan: 20,
  });
  const vItems = rowVirt.getVirtualItems();
  const padTop = vItems.length ? vItems[0].start : 0;
  const padBottom = vItems.length
    ? rowVirt.getTotalSize() - vItems[vItems.length - 1].end
    : 0;

  // revenir en haut quand le filtre / la vue / le tri change (sinon la plage
  // virtualisee peut rester au-dela des lignes filtrees et n'afficher rien)
  useEffect(() => {
    if (scrollRef.current) scrollRef.current.scrollTop = 0;
    rowVirt.scrollToOffset(0);
  }, [filter, sel, sortCol, sortAsc, dateFrom, dateTo, rowVirt]);

  const toggleSort = (c: string) => {
    if (sortCol === c) setSortAsc((v) => !v);
    else {
      setSortCol(c);
      setSortAsc(true);
    }
  };

  const exportTable = async (format: "csv" | "xlsx") => {
    if (!view || !sel) return;
    if (sel.kind === "tags" || sel.kind === "keywords" || sel.kind === "kwresults") {
      await message(
        "Export direct non disponible pour cette vue. Utilisez le rapport HTML (éléments marqués) ou une autre vue.",
        { title: "NavForensics" }
      );
      return;
    }
    try {
      const path = await save({
        filters: [{ name: format.toUpperCase(), extensions: [format] }],
      });
      if (!path) return;
      if (sel.kind === "cat") {
        await api.exportCategory(sel.ai, sel.ci, path, tz, format);
      } else {
        const term =
          sel.kind === "search"
            ? sel.term
            : sel.kind === "flagged"
            ? sel.browser
            : null;
        await api.exportView(sel.kind, term, noisy, path, tz, format);
      }
      const h = await api.sha256Of(path).catch(() => "");
      await message(`Export terminé : ${path}${h ? "\n\nSHA-256 : " + h : ""}`, {
        title: "NavForensics",
      });
    } catch (e) {
      await reportError(e);
    }
  };
  const exportReport = async (format: "html" | "pdf" = "html") => {
    try {
      const ext = format === "pdf" ? "pdf" : "html";
      const path = await save({
        filters: [{ name: format.toUpperCase(), extensions: [ext] }],
      });
      if (!path) return;
      if (format === "pdf") {
        await api.exportReportPdf(path, tz, noisy, caseInfo, reportOpts, identityPick);
      } else {
        await api.exportReport(path, tz, noisy, caseInfo, reportOpts, identityPick);
      }
      setShowReportOpts(false);
      const h = await api.sha256Of(path).catch(() => "");
      await message(`Rapport généré : ${path}${h ? "\n\nSHA-256 : " + h : ""}`, {
        title: "NavForensics",
      });
    } catch (e) {
      await reportError(e);
    }
  };

  // --- multi-sélection, menu contextuel, multi-tag ---
  const rowUrl = (r: Row): string => {
    const cell = r.values["URL"];
    return cell && cell.t === "text" ? cell.v : "";
  };
  const copyUrls = async (rows: Row[]) => {
    const urls = rows.map(rowUrl).filter(Boolean);
    try {
      await navigator.clipboard.writeText(urls.join("\n"));
    } catch {
      /* clipboard indisponible */
    }
    setCtxMenu(null);
  };
  // source (navigateur, chemin, date de modif, taille) d'une ligne à marquer
  const sourceForRow = (
    r: Row
  ): { browser: string; path: string; modified: number | null; size: number | null } => {
    if (sel?.kind === "cat") {
      const o = over[sel.ai];
      if (o) return { browser: o.browser, path: o.path, modified: o.modified, size: o.size };
    }
    // Vues agrégées : la ligne porte le chemin du fichier source (colonne cachée).
    const chemin = cellText(r.values["_chemin"]);
    if (chemin) {
      const o = over.find((x) => x.path === chemin);
      return {
        browser: cellText(r.values["Navigateur"]) || (o ? o.browser : ""),
        path: chemin,
        modified: o ? o.modified : null,
        size: o ? o.size : null,
      };
    }
    const browser = cellText(r.values["Navigateur"]);
    const srcLabel = cellText(r.values["Source"]) || cellText(r.values["Scelle"]);
    const o =
      over.find((x) => x.evidence === srcLabel) ||
      (srcLabel ? over.find((x) => srcLabel.includes(x.evidence)) : undefined);
    if (o) return { browser: browser || o.browser, path: o.path, modified: o.modified, size: o.size };
    return { browser, path: "", modified: null, size: null };
  };
  // Rend une valeur de cellule : lien cliquable (ouverture navigateur) si URL.
  const cellContent = (val: string) => {
    if (/^https?:\/\//i.test(val.trim())) {
      return (
        <a
          className="celllink"
          href={val}
          title={"Ouvrir dans le navigateur : " + val}
          onClick={(e) => {
            e.preventDefault();
            e.stopPropagation();
            api.openUrl(val.trim()).catch(() => {});
          }}
        >
          {val}
        </a>
      );
    }
    return val;
  };
  const applyTagToRows = async (rows: Row[]) => {
    if (!view || rows.length === 0) return;
    try {
      const now = new Date().toISOString();
      const entries: TagEntry[] = rows.map((r) => {
        const src = sourceForRow(r);
        const sig = sigOf(r, view.columns);
        const prev = tagBySig.get(sig);
        return {
          sig,
          label: tagLabel,
          color: tagColor,
          note: tagNote,
          context: view.title,
          summary: rowSummary(r, view.columns, tz),
          when: now,
          browser: src.browser,
          path: src.path,
          modified: src.modified,
          size: src.size,
          origin: (prev && tagOrigin(prev)) || viewOrigin(sel?.kind),
        };
      });
      await api.setTags(entries);
      await refreshTags();
      setTagModalRows(null);
      setDetail(null);
    } catch (e) {
      await reportError(e);
    }
  };
  const untagRows = async (sigs: string[]) => {
    try {
      await api.removeTags(sigs);
      await refreshTags();
      setCtxMenu(null);
    } catch (e) {
      await reportError(e);
    }
  };
  // ouvre le menu contextuel ; si la ligne n'est pas dans la sélection, elle la remplace
  const openCtx = (e: React.MouseEvent, r: Row) => {
    e.preventDefault();
    if (!view) return;
    const sig = sigOf(r, view.columns);
    if (!selected.has(sig)) setSelected(new Set([sig]));
    setCtxMenu({ x: e.clientX, y: e.clientY, row: r });
  };
  const toggleSelect = (e: React.MouseEvent, r: Row) => {
    if (!view) return;
    const sig = sigOf(r, view.columns);
    setSelected((prev) => {
      const next = e.ctrlKey || e.metaKey ? new Set(prev) : new Set<string>();
      if (next.has(sig)) next.delete(sig);
      else next.add(sig);
      return next;
    });
  };
  // lignes couramment sélectionnées (objets Row) dans la vue filtrée
  const selectedRows = (): Row[] => {
    if (!view) return [];
    return displayed.filter((r) => selected.has(sigOf(r, view.columns)));
  };

  // Convertit une ligne (vue courante) en map colonne→texte affiché,
  // pour l'envoyer au moteur comme élément d'identité retenu.
  const rowToStrMap = (r: Row): Record<string, string> => {
    const m: Record<string, string> = {};
    if (!view) return m;
    for (const c of view.columns) m[c] = fmtCell(r.values[c], tz);
    return m;
  };

  // Ajoute au rapport les éléments d'identité sélectionnés (sans doublon).
  const addIdentityToReport = (rows: Row[]) => {
    const maps = rows.map(rowToStrMap);
    setIdentityPick((prev) => {
      const seen = new Set(prev.map((m) => JSON.stringify(m)));
      const add = maps.filter((m) => !seen.has(JSON.stringify(m)));
      return [...prev, ...add];
    });
  };

  const runKeywordSearch = (items: KwItem[]) => {
    setKwQuery(items);
    setSel({ kind: "keywords" });
    setShowKw(false);
  };

  const runFxDecrypt = () => {
    setFxPwd(fxPromptVal);
    setShowFxPrompt(false);
    setSel({ kind: "fxpasswords" });
  };

  // --- marquage / annotation ---
  const refreshTags = async () => {
    try {
      setTags(await api.listTags());
      refreshResCounts();
    } catch (e) {
      await reportError(e);
    }
  };
  const applyTag = async () => {
    if (!detail || !view) return;
    try {
      const src = sourceForRow(detail);
      const sig = sigOf(detail, view.columns);
      const prev = tagBySig.get(sig);
      await api.setTag({
        sig,
        label: tagLabel,
        color: tagColor,
        note: tagNote,
        context: view.title,
        summary: rowSummary(detail, view.columns, tz),
        when: new Date().toISOString(),
        browser: src.browser,
        path: src.path,
        modified: src.modified,
        size: src.size,
        origin: (prev && tagOrigin(prev)) || viewOrigin(sel?.kind),
      });
      await refreshTags();
      setDetail(null);
    } catch (e) {
      await reportError(e);
    }
  };
  const clearTag = async () => {
    if (!detail || !view) return;
    try {
      await api.removeTag(sigOf(detail, view.columns));
      await refreshTags();
      setDetail(null);
    } catch (e) {
      await reportError(e);
    }
  };
  const saveTagsFile = async () => {
    try {
      const path = await save({
        filters: [{ name: "JSON", extensions: ["json"] }],
        defaultPath: "marquages_navforensics.json",
      });
      if (!path) return;
      await api.saveTags(path);
      await message("Marquages enregistrés : " + path, { title: "NavForensics" });
    } catch (e) {
      await reportError(e);
    }
  };
  const loadTagsFile = async () => {
    try {
      const picked = await open({
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
        title: "Charger des marquages",
      });
      if (!picked || Array.isArray(picked)) return;
      setTags(await api.loadTags(picked));
      await message("Marquages chargés.", { title: "NavForensics" });
    } catch (e) {
      await reportError(e);
    }
  };

  return (
    <div className="app">
      <div className="toolbar">
        <button className="btn" onClick={addFiles}>➕ Fichiers</button>
        <button className="btn" onClick={addFolder}>📁 Dossier</button>
        <button className="btn" onClick={loadE01} title="Extraire les artefacts des navigateurs depuis une image E01 / brute (NTFS)">💽 Image E01</button>
        <button className="btn" onClick={autodetectRun} title="Détecter et charger les profils des navigateurs de cette machine">🔎 Auto-détection</button>
        <button className="btn" onClick={addCache}>🧩 Cache</button>
        <button className="btn" onClick={loadMedia} title="Images, vidéos et pages HTML en cache">🖼 Cache média/HTML</button>
        <button className="btn" onClick={clearAll}>🗑 Vider</button>
        <div className="sep" />
        <div className="dropdown">
          <button className="btn" onClick={() => setShowExport((v) => !v)}>
            ⬇ Export ▾
          </button>
          {showExport && (
            <div className="dropmenu" onMouseLeave={() => setShowExport(false)}>
              <div
                className="dropitem"
                onClick={() => {
                  setShowExport(false);
                  exportTable("csv");
                }}
              >
                ⬇ CSV (vue courante)
              </div>
              <div
                className="dropitem"
                onClick={() => {
                  setShowExport(false);
                  exportTable("xlsx");
                }}
              >
                ⬇ XLSX (vue courante)
              </div>
              <div className="dropsep" />
              <div
                className="dropitem"
                onClick={() => {
                  setShowExport(false);
                  setShowReportOpts(true);
                }}
              >
                📄 Rapport (HTML / PDF)…
              </div>
            </div>
          )}
        </div>
        <button className="btn" onClick={() => setShowKw(true)}>🔑 Mots-clés</button>
        <button className="btn" onClick={loadIoc} title="Charger une liste de surveillance (IOC) et rechercher les correspondances">🎯 IOC</button>
        <button className="btn" onClick={() => setShowSources(true)} title="Forcer/corriger le navigateur d'un scellé">🗂 Sources</button>
        <button className="btn" onClick={() => setShowCase(true)}>Dossier…</button>
        <div className="sep" />
        <button className="btn" onClick={saveTagsFile} title="Enregistrer les marquages (JSON)">💾 Marquages</button>
        <button className="btn" onClick={loadTagsFile} title="Charger des marquages (JSON)">📂 Marquages</button>
        <div className="sep" />
        <span className="lbl">Thème :</span>
        <select value={theme} onChange={(e) => setTheme(e.target.value)}>
          {THEMES.map((t) => (
            <option key={t} value={t}>{t}</option>
          ))}
        </select>
        <span className="lbl">Fuseau :</span>
        <select value={tz} onChange={(e) => setTz(e.target.value)}>
          <option>UTC</option>
          <option>Local</option>
        </select>
        <label className="lbl">
          <input type="checkbox" checked={noisy} onChange={(e) => setNoisy(e.target.checked)} /> cookies/autofill
        </label>
        <div className="grow" />
        <span className="lbl">Recherche globale :</span>
        <input
          value={gsearch}
          onChange={(e) => setGsearch(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && runSearch()}
          placeholder="mot-clé…"
          style={{ width: 200 }}
        />
        <button className="btn" onClick={runSearch}>🔎</button>
        <button className="btn" onClick={() => setShowHelp(true)} title="Aide / emplacements des profils">❓ Aide</button>
      </div>

      <div className="body">
        <div className="nav">
          <div className="navhdr">Navigateurs / sous-catégories</div>
          {groups.length > 0 && (
            <>
              <div
                className={"node top" + (sel?.kind === "timeline" ? " sel" : "")}
                onClick={() => setSel({ kind: "timeline" })}
              >⏱ Timeline unifiée</div>
              <div
                className={"node top" + (sel?.kind === "domains" ? " sel" : "")}
                onClick={() => setSel({ kind: "domains" })}
              >🌐 Regroupement par domaine</div>
              <div
                className={
                  "node top flag" +
                  (sel?.kind === "flagged" && sel.browser === null ? " sel" : "")
                }
                onClick={() => setSel({ kind: "flagged", browser: null })}
              >🚩 Liens signalés (tous)</div>
              <div
                className={"node top" + (sel?.kind === "search_terms" ? " sel" : "")}
                onClick={() => setSel({ kind: "search_terms" })}
              >🔎 Termes recherchés</div>
              <div
                className={"node top" + (sel?.kind === "stats" ? " sel" : "")}
                onClick={() => setSel({ kind: "stats" })}
              >📊 Statistiques</div>
              <div
                className={"node top" + (sel?.kind === "histogram" ? " sel" : "")}
                onClick={() => setSel({ kind: "histogram" })}
              >📈 Histogramme d'activité</div>
              <div
                className={"node top" + (sel?.kind === "ga" ? " sel" : "")}
                onClick={() => setSel({ kind: "ga" })}
              >🍪 Cookies Google Analytics</div>
              <div
                className={"node top" + (sel?.kind === "online" ? " sel" : "")}
                onClick={() => setSel({ kind: "online" })}
              >🌐 Services en ligne</div>
              <div
                className={"node top" + (sel?.kind === "keywords" ? " sel" : "")}
                onClick={() => setShowKw(true)}
              >🔑 Recherche par mots-clés</div>
              <div className="navhdr navsub">Résultats</div>
              {(() => {
                const toggle = (k: string) =>
                  setCollapsed((prev) => {
                    const n = new Set(prev);
                    if (n.has(k)) n.delete(k);
                    else n.add(k);
                    return n;
                  });
                const idOpen = !collapsed.has("identity");
                return (
                  <>
                    <div
                      className={
                        "node top" +
                        (sel?.kind === "identity" && !sel.group && !sel.type ? " sel" : "")
                      }
                      onClick={() => setSel({ kind: "identity" })}
                    >
                      <span
                        className="chev"
                        onClick={(e) => {
                          e.stopPropagation();
                          toggle("identity");
                        }}
                      >
                        {idOpen ? "▾" : "▸"}
                      </span>
                      🪪 Éléments d'identité ({resCounts.identity ?? 0})
                    </div>
                    {idOpen &&
                      [...IDENT_GROUPS, IDENT_OTHER].map((g) => {
                        const types = Object.keys(identCounts)
                          .filter((t) => identGroupOf(t) === g.key)
                          .sort((a, b) => identCounts[b] - identCounts[a]);
                        if (types.length === 0) return null;
                        const total = types.reduce((a, t) => a + identCounts[t], 0);
                        const gKey = "identity:" + g.key;
                        const gOpen = !collapsed.has(gKey);
                        const gOn = sel?.kind === "identity" && sel.group === g.key && !sel.type;
                        return (
                          <div key={g.key}>
                            <div
                              className={"node subhdr" + (gOn ? " sel" : "")}
                              onClick={() => setSel({ kind: "identity", group: g.key })}
                            >
                              <span
                                className="chev"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  toggle(gKey);
                                }}
                              >
                                {gOpen ? "▾" : "▸"}
                              </span>
                              {g.icon} {g.label} ({total})
                            </div>
                            {gOpen &&
                              types.map((t) => {
                                const on = sel?.kind === "identity" && sel.type === t;
                                return (
                                  <div
                                    key={t}
                                    className={"node child" + (on ? " sel" : "")}
                                    onClick={() => setSel({ kind: "identity", type: t })}
                                  >
                                    {t} ({identCounts[t]})
                                  </div>
                                );
                              })}
                          </div>
                        );
                      })}
                  </>
                );
              })()}
              <div
                className={"node top" + (sel?.kind === "downloads" ? " sel" : "")}
                onClick={() => setSel({ kind: "downloads" })}
              >⬇️ Téléchargements ({resCounts.downloads ?? 0})</div>
              <div
                className={"node top" + (sel?.kind === "geolocation" ? " sel" : "")}
                onClick={() => setSel({ kind: "geolocation" })}
              >📍 Géolocalisations ({resCounts.geolocation ?? 0})</div>
              <div
                className={"node top" + (sel?.kind === "vpn" ? " sel" : "")}
                onClick={() => setSel({ kind: "vpn" })}
              >🛡️ VPN / anonymiseurs ({resCounts.vpn ?? 0})</div>
              <div
                className={"node top" + (sel?.kind === "recovered" ? " sel" : "")}
                onClick={() => setSel({ kind: "recovered" })}
                title="Enregistrements supprimés récupérés par carving SQLite"
              >♻️ Récupérés (supprimés) ({resCounts.recovered ?? 0})</div>
              <div
                className={"node top" + (sel?.kind === "crypto" ? " sel" : "")}
                onClick={() => setSel({ kind: "crypto" })}
                title="Adresses crypto, plateformes et phrases mnémoniques possibles"
              >₿ Crypto-actifs ({resCounts.crypto ?? 0})</div>
              <div
                className={"node top" + (sel?.kind === "kwresults" ? " sel" : "")}
                onClick={() => setSel({ kind: "kwresults" })}
                title="Éléments marqués issus des recherches par mots-clés et de la recherche globale"
              >🔑 Recherches / mots-clés ({tags.filter((t) => KW_ORIGINS.includes(tagOrigin(t))).length} tagués · {kwFound + Object.values(searchFound).reduce((a, b) => a + b, 0)} trouvés)</div>
              {(() => {
                const chrKeys = Object.keys(resCounts).filter((k) => k.startsWith("chr:"));
                const total =
                  (resCounts.fxpasswords ?? 0) +
                  chrKeys.reduce((a, k) => a + (resCounts[k] ?? 0), 0);
                const pwOpen = !collapsed.has("passwords");
                return (
                  <>
                    <div
                      className="node top"
                      onClick={() =>
                        setCollapsed((prev) => {
                          const n = new Set(prev);
                          if (n.has("passwords")) n.delete("passwords");
                          else n.add("passwords");
                          return n;
                        })
                      }
                      title="Identifiants enregistrés dans les navigateurs"
                    >
                      <span className="chev">{pwOpen ? "▾" : "▸"}</span>🔐 Mots de passe ({total})
                    </div>
                    {pwOpen && (
                      <>
                        <div
                          className={"node child" + (sel?.kind === "fxpasswords" ? " sel" : "")}
                          onClick={() => { setFxPromptVal(fxPwd); setShowFxPrompt(true); }}
                          title="Déchiffre les mots de passe enregistrés dans Firefox (key4.db + logins.json)"
                        >🦊 Firefox ({resCounts.fxpasswords ?? 0})</div>
                        {chrKeys.map((k) => {
                          const browser = k.slice(4);
                          const on = sel?.kind === "chrpasswords" && sel.browser === browser;
                          return (
                            <div
                              key={k}
                              className={"node child" + (on ? " sel" : "")}
                              onClick={() => setSel({ kind: "chrpasswords", browser })}
                              title="Métadonnées des identifiants Chromium : mots de passe non déchiffrés"
                            >🌐 {browser} ({resCounts[k] ?? 0})</div>
                          );
                        })}
                        <div
                          className="node child"
                          style={{ opacity: 0.6 }}
                          title="IE et Edge Legacy stockent leurs mots de passe dans le registre et le coffre Windows : non pris en charge"
                        >IE / Edge Legacy : non pris en charge</div>
                      </>
                    )}
                  </>
                );
              })()}
              <div
                className={"node top" + (sel?.kind === "ioc" ? " sel" : "")}
                onClick={() => setSel({ kind: "ioc" })}
                title="Correspondances de la liste de surveillance (IOC) chargée"
              >🎯 Correspondances IOC ({resCounts.ioc ?? 0})</div>
              <div
                className={"node top" + (sel?.kind === "sync" ? " sel" : "")}
                onClick={() => setSel({ kind: "sync" })}
                title="Comptes de synchronisation liés aux profils"
              >🔄 Comptes synchronisés ({resCounts.sync ?? 0})</div>
              <div
                className={"node top" + (sel?.kind === "extensions" ? " sel" : "")}
                onClick={() => setSel({ kind: "extensions" })}
                title="Extensions installées et leurs permissions"
              >🧩 Extensions ({resCounts.extensions ?? 0})</div>
              <div
                className={"node top" + (sel?.kind === "geopoints" ? " sel" : "")}
                onClick={() => setSel({ kind: "geopoints" })}
                title="Coordonnées GPS extraites des URLs (carte dans le rapport)"
              >🗺️ Points géolocalisés ({resCounts.geopoints ?? 0})</div>
              <div
                className={"node top" + (sel?.kind === "heatmap" ? " sel" : "")}
                onClick={() => setSel({ kind: "heatmap" })}
                title="Répartition de l'activité par jour et heure"
              >🔥 Pattern of life ({resCounts.heatmap ?? 0})</div>
              {media.length > 0 && (
                <div
                  className={"node top" + (sel?.kind === "media" ? " sel" : "")}
                  onClick={() => setSel({ kind: "media" })}
                >🖼 Galerie média ({media.length})</div>
              )}
              {htmls.length > 0 && (
                <div
                  className={"node top" + (sel?.kind === "html" ? " sel" : "")}
                  onClick={() => setSel({ kind: "html" })}
                >📄 Pages HTML ({htmls.length})</div>
              )}
              <div
                className={"node top flag" + (sel?.kind === "tags" ? " sel" : "")}
                onClick={() => setSel({ kind: "tags" })}
              >🏷 Éléments marqués ({tags.length})</div>
            </>
          )}
          {groups.map((g, gi) => {
            const badge = browserBadge(g.browser);
            const open = !collapsed.has(g.browser);
            return (
            <div key={gi}>
              <div
                className="node browser"
                onClick={() =>
                  setCollapsed((prev) => {
                    const n = new Set(prev);
                    if (n.has(g.browser)) n.delete(g.browser);
                    else n.add(g.browser);
                    return n;
                  })
                }
              >
                <span className="chev">{open ? "▾" : "▸"}</span>
                <span className="browbadge" style={{ background: badge.bg }}>{badge.ch}</span>
                {g.browser}
              </div>
              {open && g.subs.map((s, si) => {
                const subKey = g.browser + "\u0000" + s.name;
                const subOpen = !collapsed.has(subKey);
                return (
                <div key={si}>
                  <div
                    className="node subhdr"
                    onClick={() =>
                      setCollapsed((prev) => {
                        const n = new Set(prev);
                        if (n.has(subKey)) n.delete(subKey);
                        else n.add(subKey);
                        return n;
                      })
                    }
                  >
                    <span className="chev">{subOpen ? "▾" : "▸"}</span>
                    {s.name}
                  </div>
                  {subOpen && s.items.map((it, ii) => {
                    const on =
                      it.kind === "cat"
                        ? sel?.kind === "cat" && sel.ai === it.ai && sel.ci === it.ci
                        : sel?.kind === "flagged" && sel.browser === it.browser;
                    return (
                      <div
                        key={ii}
                        className={
                          "node child" +
                          (it.kind === "flagged" ? " flag" : "") +
                          (on ? " sel" : "")
                        }
                        onClick={() => openItem(it)}
                      >
                        {it.label}{" "}
                        {it.kind === "cat" && (
                          <span className="count">({it.count})</span>
                        )}
                      </div>
                    );
                  })}
                </div>
                );
              })}
            </div>
          );
          })}
          {groups.length === 0 && (
            <div className="navhdr">Aucune source. Utilisez ➕ Fichiers ou 📁 Dossier.</div>
          )}
        </div>

        <div className="right">
          <div className="filterbar">
            <span className="lbl">Filtre :</span>
            <input value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="filtrer la vue…" />
            <button className="btn" onClick={() => setFilter("")}>✖</button>
            <span className="sep" />
            <span className="lbl">Du :</span>
            <input type="date" value={dateFrom} onChange={(e) => setDateFrom(e.target.value)} />
            <span className="lbl">au :</span>
            <input type="date" value={dateTo} onChange={(e) => setDateTo(e.target.value)} />
            <button
              className="btn"
              title="Effacer la plage de dates"
              onClick={() => {
                setDateFrom("");
                setDateTo("");
              }}
            >✖ dates</button>
            <span className="entries">
              {view ? `${displayed.length} / ${view.rows.length} entrées` : ""}
            </span>
          </div>

          <div className="tablewrap" ref={scrollRef}>
            {sel?.kind === "geopoints" && (
              <GeoMap points={geoPoints} onOpen={(u) => api.openUrl(u).catch(() => {})} />
            )}
            {showTimeline && timeExtent && (
              <TimelineBrush
                events={allEvents}
                extent={timeExtent}
                win={timeWin ?? timeExtent}
                sel={timeSel}
                tz={tz}
                onWin={setTimeWin}
                onSel={setTimeSel}
                onReset={() => {
                  setTimeWin(null);
                  setTimeSel(null);
                }}
              />
            )}
            {sel?.kind === "media" && (
              <div className="gallery">
                {media.length === 0 && (
                  <div className="navhdr">Aucun média. Utilisez « 🖼 Média cache » sur un dossier de cache/profil.</div>
                )}
                {media.map((m, i) => (
                  <div className="mediacard" key={i}>
                    <div className="mediathumb" onClick={() => setZoomMedia(m)}>
                      {m.mime.startsWith("image/") ? (
                        <img src={`data:${m.mime};base64,${m.data}`} loading="lazy" alt={m.name} />
                      ) : (
                        <video src={`data:${m.mime};base64,${m.data}`} muted />
                      )}
                    </div>
                    <div className="mediameta">
                      <span className="medianame" title={m.name}>{m.name}</span>
                      <span className="mediasub">{m.mime} · {(m.size / 1024).toFixed(0)} Ko</span>
                      <button className="btn" title="Exporter" onClick={() => saveMediaItem(m)}>💾</button>
                    </div>
                  </div>
                ))}
              </div>
            )}
            {sel?.kind === "html" && (
              <div className="htmllist">
                {htmls.length === 0 && (
                  <div className="navhdr">Aucune page HTML. Utilisez « 🖼 Cache média/HTML ».</div>
                )}
                {htmls.map((h, i) => (
                  <div className="htmlrow" key={i} onClick={() => setHtmlView(h)}>
                    <div className="htmltitle">{h.title || "(sans titre)"}</div>
                    <div className="htmlsub">
                      {h.name} · {(h.size / 1024).toFixed(0)} Ko · {h.resources.length} ressources
                    </div>
                  </div>
                ))}
              </div>
            )}
            {view && sel?.kind !== "media" && sel?.kind !== "html" && (
              <table style={{ width: totalWidth, tableLayout: "fixed" }}>
                <colgroup>
                  {view.columns.map((c, i) => (
                    <col key={c} style={{ width: widths[i] }} />
                  ))}
                </colgroup>
                <thead>
                  <tr>
                    {view.columns.map((c, idx) => (
                      <th
                        key={c}
                        onClick={() => toggleSort(c)}
                        style={{ position: "relative" }}
                      >
                        {c}
                        {sortCol === c ? (sortAsc ? " ▴" : " ▾") : ""}
                        <span
                          className="col-resizer"
                          onMouseDown={(e) => onResizeStart(e, idx)}
                          onDoubleClick={(e) => onResizeReset(e, idx)}
                          onClick={(e) => e.stopPropagation()}
                          title="Glisser pour redimensionner · double-clic pour réinitialiser"
                          style={{
                            position: "absolute",
                            top: 0,
                            right: 0,
                            width: 7,
                            height: "100%",
                            cursor: "col-resize",
                            userSelect: "none",
                          }}
                        />
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {padTop > 0 && <tr style={{ height: padTop }} />}
                  {vItems.map((vi) => {
                    const r = displayed[vi.index];
                    if (!r) return null; // plage virtualisee transitoirement hors bornes (apres filtrage)
                    const sig = sigOf(r, view.columns);
                    const tg = tagBySig.get(sig);
                    const isSel = selected.has(sig);
                    return (
                      <tr
                        key={vi.index}
                        className={
                          (tg ? "tagged" : "") + (isSel ? " selected" : "")
                        }
                        title={tg ? `${tg.label}${tg.note ? " — " + tg.note : ""}` : undefined}
                        onClick={(e) => toggleSelect(e, r)}
                        onDoubleClick={() => setDetail(r)}
                        onContextMenu={(e) => openCtx(e, r)}
                      >
                        {view.columns.map((c, ci) => (
                          <td
                            key={c}
                            title={fmtCell(r.values[c], tz)}
                            style={
                              ci === 0 && tg
                                ? { boxShadow: `inset 4px 0 0 ${tg.color || "#f59e0b"}` }
                                : undefined
                            }
                          >
                            {ci === 0 && tg ? (
                              <span
                                style={{
                                  display: "inline-block",
                                  width: 8,
                                  height: 8,
                                  borderRadius: 2,
                                  background: tg.color || "#f59e0b",
                                  marginRight: 6,
                                  verticalAlign: "middle",
                                }}
                              />
                            ) : null}
                            {cellContent(fmtCell(r.values[c], tz))}
                          </td>
                        ))}
                      </tr>
                    );
                  })}
                  {padBottom > 0 && <tr style={{ height: padBottom }} />}
                </tbody>
              </table>
            )}
          </div>

          <div className="status">
            {view ? `${view.title} — SHA-256 : ` : "Prêt."}
            <span className="mono">{view?.sha ?? ""}</span>
          </div>
        </div>
      </div>

      {detail && view && (
        <div className="overlay" onClick={() => setDetail(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Détail de l'entrée</h3>
            {view.columns.map((c) => (
              <div className="field" key={c}>
                <div className="k">{c}</div>
                <div className="v">{fmtCell(detail.values[c], tz)}</div>
              </div>
            ))}
            {detail.raw.length > 0 && (
              <>
                <hr />
                <div className="k">Valeurs brutes (non converties)</div>
                {detail.raw.map(([k, v], i) => (
                  <div className="field" key={i}>
                    <span className="k">{k}</span> = <span className="mono">{v}</span>
                  </div>
                ))}
              </>
            )}
            <hr />
            <div className="k">Source</div>
            {(() => {
              const s = sourceForRow(detail);
              return (
                <>
                  <div className="field">
                    <div className="k">Navigateur</div>
                    <div className="v">{s.browser || "—"}</div>
                  </div>
                  <div className="field">
                    <div className="k">Chemin complet</div>
                    <div className="v mono">{s.path || "— (vue agrégée, chemin indisponible)"}</div>
                  </div>
                  <div className="field">
                    <div className="k">Dernière modif.</div>
                    <div className="v">
                      {s.modified != null ? fmtCell({ t: "time", v: s.modified }, tz) : "—"}
                    </div>
                  </div>
                  <div className="field">
                    <div className="k">Taille</div>
                    <div className="v">
                      {s.size != null ? `${(s.size / 1024).toFixed(0)} Ko (${s.size} o)` : "—"}
                    </div>
                  </div>
                </>
              );
            })()}
            <hr />
            <div className="k">Marquage d'investigation</div>
            <div className="tagbox">
              <div className="tagrow">
                <select
                  value={TAG_PRESETS.find((x) => x.name === tagLabel)?.name ?? ""}
                  onChange={(e) => {
                    const p = TAG_PRESETS.find((x) => x.name === e.target.value);
                    if (p) {
                      setTagLabel(p.name);
                      setTagColor(p.color);
                    }
                  }}
                >
                  <option value="">— modèle —</option>
                  {TAG_PRESETS.map((p) => (
                    <option key={p.name} value={p.name}>{p.name}</option>
                  ))}
                </select>
                <input
                  type="color"
                  value={tagColor}
                  onChange={(e) => setTagColor(e.target.value)}
                  title="Couleur de la balise"
                />
                <input
                  type="text"
                  value={tagLabel}
                  onChange={(e) => setTagLabel(e.target.value)}
                  placeholder="Nom de la balise"
                  style={{ flex: 1 }}
                />
              </div>
              <textarea
                value={tagNote}
                onChange={(e) => setTagNote(e.target.value)}
                placeholder="Annotation (facultatif)…"
                rows={2}
              />
              <div className="tagbtns">
                <button className="btn" onClick={applyTag}>🏷 Marquer</button>
                {tagBySig.has(sigOf(detail, view.columns)) && (
                  <button className="btn" onClick={clearTag}>Retirer le marquage</button>
                )}
                <button className="btn" onClick={() => setDetail(null)}>Fermer</button>
              </div>
            </div>
          </div>
        </div>
      )}

      {showCase && (
        <div className="overlay" onClick={() => setShowCase(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Informations du dossier</h3>
            {([
              ["Numéro de dossier / PV", "numero"],
              ["Examinateur", "examinateur"],
              ["Service", "service"],
              ["Observations", "notes"],
            ] as [string, keyof CaseInfo][]).map(([lab, key]) => (
              <div className="row" key={key}>
                <label>{lab}</label>
                <input
                  value={caseInfo[key]}
                  onChange={(e) => setCaseInfo({ ...caseInfo, [key]: e.target.value })}
                />
              </div>
            ))}
            <button className="btn" onClick={() => setShowCase(false)}>Enregistrer</button>
          </div>
        </div>
      )}

      {showHelp && (
        <div className="overlay" onClick={() => setShowHelp(false)}>
          <div className="modal wide" onClick={(e) => e.stopPropagation()}>
            <h3>NavForensics — Aide</h3>
            <div className="field">
              <div className="k">Logiciel</div>
              <div className="v">NavForensics v{appVersion || "?"}</div>
            </div>
            <div className="field">
              <div className="k">Auteur</div>
              <div className="v">SCHMITT Franck</div>
            </div>
            <hr />
            <div className="k">Guide d'utilisation</div>
            {HELP_GUIDE.map((g) => (
              <div className="field" key={g.t}>
                <div className="k">{g.t}</div>
                <div className="v">
                  {g.items.map((it, i) => (
                    <div key={i}>• {it}</div>
                  ))}
                </div>
              </div>
            ))}
            <hr />
            <div className="field">
              <div className="k">Principe forensique</div>
              <div className="v">
                Lecture seule sur une copie (fichiers -wal/-shm inclus), empreinte SHA-256,
                horodatages convertis depuis chaque référentiel navigateur. Déchiffrement des
                identifiants Firefox disponible (key4.db + logins.json, sur copie).
              </div>
            </div>
            <hr />
            <div className="k">Chargement des artefacts</div>
            <div className="v">
              • <b>Auto-détection</b> : charge les profils des navigateurs installés sur <b>cette machine</b> (poste d'analyse) — fermez les navigateurs pour éviter les verrous.<br />
              • <b>Dossier</b> : analyse récursivement un dossier contenant des profils/artefacts extraits (X-Ways…).<br />
              • <b>Fichiers</b> : charge une ou plusieurs bases précises.
            </div>
            <hr />
            <div className="k">Emplacements des profils — Bureau (Windows / macOS / Linux)</div>
            <div className="v" style={{ marginBottom: 6 }}>
              Windows XP : remplacer <span className="mono">%LOCALAPPDATA%</span> par <span className="mono">%USERPROFILE%\Local Settings\Application Data</span> et <span className="mono">%APPDATA%</span> par <span className="mono">%USERPROFILE%\Application Data</span>. Chemins identiques de Windows Vista à 11 (26H2).
            </div>
            <table className="helptab">
              <thead>
                <tr><th>Navigateur</th><th>Windows</th><th>macOS / Linux</th></tr>
              </thead>
              <tbody>
                {HELP_LOCATIONS.map((h) => (
                  <tr key={h.b}>
                    <td>{h.b}</td>
                    <td className="mono">{h.win}</td>
                    <td>{h.other}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div className="k" style={{ marginTop: 10 }}>Emplacements des profils — Mobile (iOS 14→27 / Android 12→16)</div>
            <div className="v" style={{ marginBottom: 6 }}>
              Nécessite une extraction de système de fichiers complète (Cellebrite FFS, GrayKey, checkm8 pour iOS ; extraction physique / FFS pour Android), puis « Dossier » pour scanner l'arborescence montée. iOS : conteneur d'app sous <span className="mono">/private/var/mobile/Containers/Data/Application/&lt;GUID&gt;/</span>. Android : <span className="mono">/data/data/&lt;paquet&gt;/</span>.
            </div>
            <table className="helptab">
              <thead>
                <tr><th>Navigateur</th><th>iOS (bundle → base)</th><th>Android (paquet → base)</th></tr>
              </thead>
              <tbody>
                {HELP_MOBILE.map((h) => (
                  <tr key={h.b}>
                    <td>{h.b}</td>
                    <td className="mono">{h.ios}</td>
                    <td className="mono">{h.android}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div className="field">
              <div className="k">Fichiers clés</div>
              <div className="v">{HELP_FILES}</div>
            </div>
            <button className="btn" onClick={() => setShowHelp(false)}>Fermer</button>
          </div>
        </div>
      )}

      {showFxPrompt && (
        <div className="overlay" onClick={() => setShowFxPrompt(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Mots de passe Firefox — déchiffrement</h3>
            <div className="v" style={{ marginBottom: 8 }}>
              Déchiffre les identifiants enregistrés (<span className="mono">key4.db</span> + <span className="mono">logins.json</span>). Laissez le champ vide s'il n'y a pas de mot de passe maître. Lecture seule sur copie.
            </div>
            <label className="k">Mot de passe maître (optionnel)</label>
            <input
              type="password"
              value={fxPromptVal}
              onChange={(e) => setFxPromptVal(e.target.value)}
              placeholder="(vide si aucun)"
              autoFocus
              onKeyDown={(e) => { if (e.key === "Enter") runFxDecrypt(); }}
            />
            <div className="v" style={{ margin: "8px 0", fontSize: 12 }}>
              Profil : {fxDir ? <span className="mono">{fxDir}</span> : "profils Firefox chargés (auto)"}
            </div>
            <div className="tagbtns">
              <button
                className="btn"
                onClick={async () => {
                  const d = await open({ directory: true, title: "Dossier de profil Firefox (key4.db + logins.json)" });
                  if (d && !Array.isArray(d)) setFxDir(d);
                }}
              >📁 Choisir un profil…</button>
              {fxDir && <button className="btn" onClick={() => setFxDir(null)}>↺ Auto</button>}
              <button className="btn" onClick={runFxDecrypt}>🔓 Déchiffrer</button>
              <button className="btn" onClick={() => setShowFxPrompt(false)}>Annuler</button>
            </div>
          </div>
        </div>
      )}

      {showSources && (
        <div className="overlay" onClick={() => setShowSources(false)}>
          <div className="modal wide" onClick={(e) => e.stopPropagation()}>
            <h3>Sources chargées — forcer le navigateur</h3>
            <div className="note">
              Corrige le navigateur d'un scellé quand le chemin ne permet pas de le déduire
              (fichier isolé). Le changement reclasse l'arborescence et s'applique au rapport.
            </div>
            {over.length === 0 && <div className="navhdr">Aucune source chargée.</div>}
            {over.map((o, ai) => (
              <div className="srcrow" key={ai}>
                <div className="srcinfo">
                  <div className="srcname">🗂 {o.evidence} <span className="count">[{o.kind}]</span></div>
                  <div className="srcpath mono">{o.path || "—"}</div>
                  <div className="srcmeta mono">
                    Version : {o.version || "—"} · Installé :{" "}
                    {o.installed != null
                      ? fmtCell({ t: "time", v: o.installed }, tz)
                      : "—"}
                  </div>
                </div>
                <input
                  list="browserlist"
                  value={browserDraft[ai] ?? o.browser}
                  onChange={(e) => setBrowserDraft({ ...browserDraft, [ai]: e.target.value })}
                />
                <button className="btn" onClick={() => applyBrowser(ai)}>Appliquer</button>
              </div>
            ))}
            <datalist id="browserlist">
              {BROWSER_PRESETS.map((b) => (
                <option key={b} value={b} />
              ))}
            </datalist>
            <button className="btn" onClick={() => setShowSources(false)}>Fermer</button>
          </div>
        </div>
      )}

      {ctxMenu && view && (
        <div
          className="ctxoverlay"
          onClick={() => setCtxMenu(null)}
          onContextMenu={(e) => {
            e.preventDefault();
            setCtxMenu(null);
          }}
        >
          <div
            className="ctxmenu"
            style={{ left: ctxMenu.x, top: ctxMenu.y }}
            onClick={(e) => e.stopPropagation()}
          >
            <div
              className="ctxitem"
              onClick={() => {
                const rows = selected.size > 0 ? selectedRows() : [ctxMenu.row];
                setTagModalRows(rows);
                setCtxMenu(null);
              }}
            >
              🏷 Marquer{selected.size > 1 ? ` la sélection (${selected.size})` : "…"}
            </div>
            {sel?.kind === "identity" && (
              <div
                className="ctxitem"
                onClick={() => {
                  const rows = selected.size > 0 ? selectedRows() : [ctxMenu.row];
                  addIdentityToReport(rows);
                  setCtxMenu(null);
                }}
              >
                ➕ Ajouter au rapport
                {selected.size > 1 ? ` (${selected.size})` : ""}
              </div>
            )}
            <div
              className="ctxitem"
              onClick={() =>
                untagRows(
                  selected.size > 0
                    ? Array.from(selected)
                    : [sigOf(ctxMenu.row, view.columns)]
                )
              }
            >
              ✖ Retirer le marquage
            </div>
            <div
              className="ctxitem"
              onClick={() => copyUrls(selected.size > 0 ? selectedRows() : [ctxMenu.row])}
            >
              📋 Copier l'URL{selected.size > 1 ? "s" : ""}
            </div>
            <div
              className="ctxitem"
              onClick={() => {
                setDetail(ctxMenu.row);
                setCtxMenu(null);
              }}
            >
              🔍 Détails
            </div>
          </div>
        </div>
      )}

      {tagModalRows && (
        <div className="overlay" onClick={() => setTagModalRows(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Marquer {tagModalRows.length} élément{tagModalRows.length > 1 ? "s" : ""}</h3>
            <div className="tagbox">
              <div className="tagrow">
                <select
                  value={TAG_PRESETS.find((x) => x.name === tagLabel)?.name ?? ""}
                  onChange={(e) => {
                    const p = TAG_PRESETS.find((x) => x.name === e.target.value);
                    if (p) {
                      setTagLabel(p.name);
                      setTagColor(p.color);
                    }
                  }}
                >
                  <option value="">— modèle —</option>
                  {TAG_PRESETS.map((p) => (
                    <option key={p.name} value={p.name}>{p.name}</option>
                  ))}
                </select>
                <input type="color" value={tagColor} onChange={(e) => setTagColor(e.target.value)} />
                <input
                  type="text"
                  value={tagLabel}
                  onChange={(e) => setTagLabel(e.target.value)}
                  placeholder="Nom de la balise"
                  style={{ flex: 1 }}
                />
              </div>
              <textarea
                value={tagNote}
                onChange={(e) => setTagNote(e.target.value)}
                placeholder="Annotation (facultatif)…"
                rows={2}
              />
              <div className="tagbtns">
                <button className="btn" onClick={() => applyTagToRows(tagModalRows)}>🏷 Appliquer</button>
                <button className="btn" onClick={() => setTagModalRows(null)}>Annuler</button>
              </div>
            </div>
          </div>
        </div>
      )}

      {showReportOpts && (
        <div className="overlay" onClick={() => setShowReportOpts(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Options du rapport</h3>
            <label className="optrow">
              <input
                type="checkbox"
                checked={reportOpts.only_tagged}
                onChange={(e) => setReportOpts({ ...reportOpts, only_tagged: e.target.checked })}
              />{" "}
              Uniquement les éléments marqués (+ données du dossier)
            </label>
            <hr />
            <div className="k">Sections à inclure</div>
            {(
              [
                ["include_tags", "Éléments marqués"],
                ["include_stats", "Statistiques"],
                ["include_histogram", "Histogramme"],
                ["include_timeline", "Timeline"],
                ["include_domains", "Domaines"],
                ["include_terms", "Termes recherchés"],
                ["include_online", "Services en ligne"],
                ["include_ga", "Cookies Google Analytics"],
                ["include_flagged", "Liens signalés"],
                ["include_identity", "Éléments d'identité (autofill)"],
                ["include_downloads", "Téléchargements"],
                ["include_geolocation", "Géolocalisations"],
                ["include_vpn", "VPN"],
                ["include_recovered", "Enregistrements supprimés (récupérés)"],
                ["include_crypto", "Crypto-actifs"],
                ["include_ioc", "Correspondances IOC"],
                ["include_sync", "Comptes synchronisés"],
                ["include_extensions", "Extensions"],
                ["include_map", "Carte des géolocalisations"],
                ["include_heatmap", "Pattern of life (heatmap)"],
                ["include_details", "Détail par navigateur"],
              ] as [keyof ReportOptions, string][]
            ).map(([k, lab]) => (
              <label key={k} className="optrow">
                <input
                  type="checkbox"
                  disabled={reportOpts.only_tagged}
                  checked={reportOpts[k]}
                  onChange={(e) => setReportOpts({ ...reportOpts, [k]: e.target.checked })}
                />{" "}
                {lab}
              </label>
            ))}
            <hr />
            <div className="optrow">
              <span className="k">Éléments d'identité retenus : </span>
              {identityPick.length === 0 ? (
                <span className="muted">
                  aucun (clic droit sur la vue Identité → « Ajouter au rapport »)
                </span>
              ) : (
                <>
                  <b>{identityPick.length}</b> sélectionné
                  {identityPick.length > 1 ? "s" : ""}{" "}
                  <button className="btn small" onClick={() => setIdentityPick([])}>
                    Vider
                  </button>
                </>
              )}
            </div>
            <div className="tagbtns">
              <button className="btn" onClick={() => exportReport("html")}>
                📄 Générer HTML
              </button>
              <button className="btn" onClick={() => exportReport("pdf")}>
                📑 Générer PDF
              </button>
              <button className="btn" onClick={() => setShowReportOpts(false)}>
                Fermer
              </button>
            </div>
          </div>
        </div>
      )}

      {showKw && (
        <div className="overlay" onClick={() => setShowKw(false)}>
          <div className="modal wide" onClick={(e) => e.stopPropagation()}>
            <h3>Listes de mots-clés forensiques</h3>
            <div className="kwwrap">
              <div className="kwlists">
                {kwLists.map((l, i) => (
                  <div
                    key={i}
                    className={"kwlistitem" + (i === kwSel ? " sel" : "")}
                    onClick={() => setKwSel(i)}
                  >
                    {l.name} <span className="count">({l.words.length})</span>
                  </div>
                ))}
                <div className="tagbtns" style={{ marginTop: 6 }}>
                  <button
                    className="btn"
                    onClick={() => {
                      setKwLists([...kwLists, { name: "Nouvelle liste", words: [] }]);
                      setKwSel(kwLists.length);
                    }}
                  >
                    ＋ Liste
                  </button>
                  <button
                    className="btn"
                    onClick={() => setKwLists(DEFAULT_KW_LISTS)}
                    title="Réinitialiser aux listes par défaut"
                  >
                    ↺ Défaut
                  </button>
                </div>
              </div>
              <div className="kwedit">
                {kwLists[kwSel] && (
                  <>
                    <input
                      type="text"
                      value={kwLists[kwSel].name}
                      onChange={(e) =>
                        setKwLists(kwLists.map((l, i) => (i === kwSel ? { ...l, name: e.target.value } : l)))
                      }
                      placeholder="Nom de la liste"
                    />
                    <textarea
                      rows={10}
                      value={kwLists[kwSel].words.join("\n")}
                      onChange={(e) =>
                        setKwLists(
                          kwLists.map((l, i) =>
                            i === kwSel ? { ...l, words: e.target.value.split("\n") } : l
                          )
                        )
                      }
                      placeholder="un mot-clé par ligne…"
                    />
                    <div className="tagbtns">
                      <button
                        className="btn"
                        onClick={() =>
                          runKeywordSearch(
                            kwLists[kwSel].words
                              .map((w) => w.trim())
                              .filter(Boolean)
                              .map((w) => ({ word: w, list: kwLists[kwSel].name }))
                          )
                        }
                      >
                        🔑 Rechercher cette liste
                      </button>
                      <button
                        className="btn"
                        onClick={() =>
                          runKeywordSearch(
                            kwLists.flatMap((l) =>
                              l.words.map((w) => w.trim()).filter(Boolean).map((w) => ({ word: w, list: l.name }))
                            )
                          )
                        }
                      >
                        🔎 Toutes les listes
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          const next = kwLists.filter((_, i) => i !== kwSel);
                          setKwLists(next.length ? next : DEFAULT_KW_LISTS);
                          setKwSel(0);
                        }}
                      >
                        🗑 Supprimer la liste
                      </button>
                    </div>
                  </>
                )}
              </div>
            </div>
            <button className="btn" onClick={() => setShowKw(false)}>Fermer</button>
          </div>
        </div>
      )}

      {htmlView && (
        <div className="overlay" onClick={() => setHtmlView(null)}>
          <div className="modal wide" onClick={(e) => e.stopPropagation()}>
            <h3>
              {htmlView.title || htmlView.name}{" "}
              <span className="count">({(htmlView.size / 1024).toFixed(0)} Ko)</span>
            </h3>
            <div className="note">
              Code source brut, affiché en lecture seule : aucun script n'est exécuté et la page n'est pas rendue.
            </div>
            <div className="k">Ressources liées ({htmlView.resources.length})</div>
            <div className="htmlres">
              <table className="helptab">
                <thead>
                  <tr><th>Type</th><th>URL</th></tr>
                </thead>
                <tbody>
                  {htmlView.resources.map((r, i) => (
                    <tr key={i}>
                      <td>{r.kind}</td>
                      <td className="mono">{r.url}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className="k">Code source</div>
            <pre className="htmlsrc">{htmlView.source}</pre>
            <button className="btn" onClick={() => setHtmlView(null)}>Fermer</button>
          </div>
        </div>
      )}

      {zoomMedia && (
        <div className="overlay" onClick={() => setZoomMedia(null)}>
          <div className="modal wide" onClick={(e) => e.stopPropagation()}>
            <h3>
              {zoomMedia.name} <span className="count">({zoomMedia.mime})</span>
            </h3>
            <div className="zoomwrap">
              {zoomMedia.mime.startsWith("image/") ? (
                <img src={`data:${zoomMedia.mime};base64,${zoomMedia.data}`} alt={zoomMedia.name} />
              ) : (
                <video src={`data:${zoomMedia.mime};base64,${zoomMedia.data}`} controls />
              )}
            </div>
            <div className="tagbtns">
              <button className="btn" onClick={() => saveMediaItem(zoomMedia)}>💾 Exporter</button>
              <button className="btn" onClick={() => setZoomMedia(null)}>Fermer</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
