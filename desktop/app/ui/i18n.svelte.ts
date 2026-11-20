// Traductions de l'interface. Les clés sont les textes français (comme le catalogue de l'app Mac,
// Resources/Localizable.xcstrings) ; desktop/scripts/l10n.py écrit locales/<langue>.json à partir de ce catalogue
// et de locales/windows.json (textes propres à Windows).
import en from "./locales/en.json";
import nl from "./locales/nl.json";
import de from "./locales/de.json";
import es from "./locales/es.json";

const tables: Record<string, Record<string, string>> = { en, nl, de, es };
export const languages = ["fr", "en", "nl", "de", "es"] as const;

let current = $state("fr");

/** Langue choisie dans les réglages, sinon celle de Windows, sinon le français. */
export function setLanguage(choice: string | null | undefined) {
  const wanted = (choice || navigator.language || "fr").slice(0, 2).toLowerCase();
  current = (languages as readonly string[]).includes(wanted) ? wanted : "en";
}

export function language() {
  return current;
}

/** Texte traduit ; `%@`, `%lld` et `%1$@` reçoivent les valeurs dans l'ordre. */
export function t(key: string, ...args: (string | number)[]): string {
  const text = current === "fr" ? key : (tables[current]?.[key] ?? key);
  let i = 0;
  return text.replace(/%(?:(\d+)\$)?(@|lld|d)/g, (_m, pos) => {
    const value = pos ? args[Number(pos) - 1] : args[i++];
    return value === undefined ? "" : String(value);
  });
}

/** Marque un texte à traduire plus tard (listes de libellés), traduit ensuite par `t`. */
export const k = (key: string) => key;

/** Nom d'une langue dans la langue de l'interface (« fr » → « français ») ; vide si le code est inconnu. */
export function languageName(code: string | null | undefined): string {
  const c = code?.trim();
  if (!c) return "";
  try {
    const name = new Intl.DisplayNames([current], { type: "language" }).of(c);
    return name && name.toLowerCase() !== c.toLowerCase() ? name : "";
  } catch {
    return "";
  }
}

/** Taille lisible (« 2,4 Mo »). */
export function bytes(n: number | null | undefined): string {
  if (n == null) return "—";
  const units = ["o", "Ko", "Mo", "Go", "To"];
  let v = n;
  let u = 0;
  while (v >= 1000 && u < units.length - 1) {
    v /= 1000;
    u++;
  }
  return new Intl.NumberFormat(current, { maximumFractionDigits: u > 1 ? 1 : 0 }).format(v) + " " + units[u];
}

/** Date d'un horodatage (secondes depuis 2001, comme l'index de l'app Mac). */
export function date(seconds: number | null | undefined): string {
  if (seconds == null) return "—";
  return new Intl.DateTimeFormat(current, { dateStyle: "medium" }).format(new Date((seconds + 978307200) * 1000));
}
