//! Entités HTML et texte sans balises (`HTMLEntities` et `stripHTML` du cœur Swift).

use regex::{Captures, Regex};
use std::sync::LazyLock;

/// Entités HTML nommées courantes, absentes du XML pur.
const TABLE: &[(&str, u32)] = &[
    ("nbsp", 160),
    ("iexcl", 161),
    ("cent", 162),
    ("pound", 163),
    ("euro", 8364),
    ("yen", 165),
    ("sect", 167),
    ("uml", 168),
    ("copy", 169),
    ("ordf", 170),
    ("laquo", 171),
    ("not", 172),
    ("shy", 173),
    ("reg", 174),
    ("deg", 176),
    ("plusmn", 177),
    ("sup2", 178),
    ("sup3", 179),
    ("acute", 180),
    ("micro", 181),
    ("para", 182),
    ("middot", 183),
    ("cedil", 184),
    ("sup1", 185),
    ("ordm", 186),
    ("raquo", 187),
    ("frac14", 188),
    ("frac12", 189),
    ("frac34", 190),
    ("iquest", 191),
    ("Agrave", 192),
    ("Aacute", 193),
    ("Acirc", 194),
    ("Atilde", 195),
    ("Auml", 196),
    ("Aring", 197),
    ("AElig", 198),
    ("Ccedil", 199),
    ("Egrave", 200),
    ("Eacute", 201),
    ("Ecirc", 202),
    ("Euml", 203),
    ("Igrave", 204),
    ("Iacute", 205),
    ("Icirc", 206),
    ("Iuml", 207),
    ("ETH", 208),
    ("Ntilde", 209),
    ("Ograve", 210),
    ("Oacute", 211),
    ("Ocirc", 212),
    ("Otilde", 213),
    ("Ouml", 214),
    ("times", 215),
    ("Oslash", 216),
    ("Ugrave", 217),
    ("Uacute", 218),
    ("Ucirc", 219),
    ("Uuml", 220),
    ("Yacute", 221),
    ("THORN", 222),
    ("szlig", 223),
    ("agrave", 224),
    ("aacute", 225),
    ("acirc", 226),
    ("atilde", 227),
    ("auml", 228),
    ("aring", 229),
    ("aelig", 230),
    ("ccedil", 231),
    ("egrave", 232),
    ("eacute", 233),
    ("ecirc", 234),
    ("euml", 235),
    ("igrave", 236),
    ("iacute", 237),
    ("icirc", 238),
    ("iuml", 239),
    ("eth", 240),
    ("ntilde", 241),
    ("ograve", 242),
    ("oacute", 243),
    ("ocirc", 244),
    ("otilde", 245),
    ("ouml", 246),
    ("divide", 247),
    ("oslash", 248),
    ("ugrave", 249),
    ("uacute", 250),
    ("ucirc", 251),
    ("uuml", 252),
    ("yacute", 253),
    ("thorn", 254),
    ("yuml", 255),
    ("OElig", 338),
    ("oelig", 339),
    ("Scaron", 352),
    ("scaron", 353),
    ("Yuml", 376),
    ("fnof", 402),
    ("circ", 710),
    ("tilde", 732),
    ("ensp", 8194),
    ("emsp", 8195),
    ("thinsp", 8201),
    ("zwnj", 8204),
    ("zwj", 8205),
    ("lrm", 8206),
    ("rlm", 8207),
    ("ndash", 8211),
    ("mdash", 8212),
    ("lsquo", 8216),
    ("rsquo", 8217),
    ("sbquo", 8218),
    ("ldquo", 8220),
    ("rdquo", 8221),
    ("bdquo", 8222),
    ("dagger", 8224),
    ("Dagger", 8225),
    ("bull", 8226),
    ("hellip", 8230),
    ("permil", 8240),
    ("prime", 8242),
    ("Prime", 8243),
    ("lsaquo", 8249),
    ("rsaquo", 8250),
    ("trade", 8482),
    ("larr", 8592),
    ("rarr", 8594),
    ("uarr", 8593),
    ("darr", 8595),
    ("harr", 8596),
    ("minus", 8722),
    ("le", 8804),
    ("ge", 8805),
    ("ne", 8800),
    ("infin", 8734),
    ("alpha", 945),
    ("beta", 946),
    ("gamma", 947),
    ("delta", 948),
    ("pi", 960),
    ("sigma", 963),
    ("omega", 969),
    ("Omega", 937),
];

static NAMED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&([A-Za-z][A-Za-z0-9]*);").unwrap());
static NUMERIC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&#(x?)([0-9A-Fa-f]+);").unwrap());
static BLOCKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<(br|/p|/div|/li|/h[1-6])[^>]*>").unwrap());
static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

/// Remplace les entités nommées (hors entités XML) par des références numériques : le XML devient lisible.
pub fn to_numeric(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    NAMED
        .replace_all(s, |c: &Captures| {
            let name = &c[1];
            if ["amp", "lt", "gt", "quot", "apos"].contains(&name) {
                return c[0].to_string();
            }
            match TABLE.iter().find(|(n, _)| *n == name) {
                Some((_, code)) => format!("&#{code};"),
                None => format!("&amp;{name};"),
            }
        })
        .into_owned()
}

/// Décode toutes les entités vers du texte brut.
pub fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let out = to_numeric(s).replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'");
    let out = NUMERIC.replace_all(&out, |c: &Captures| {
        let value = if c[1].is_empty() { c[2].parse::<u32>().ok() } else { u32::from_str_radix(&c[2], 16).ok() };
        value.and_then(char::from_u32).map(String::from).unwrap_or_default()
    });
    out.replace("&amp;", "&")
}

/// Texte d'un résumé en HTML : balises retirées (celles de bloc séparent des mots), entités décodées, blancs réduits.
pub fn strip_html(s: &str) -> String {
    let blocks = BLOCKS.replace_all(s, " ");
    let no_tags = TAGS.replace_all(&blocks, "");
    SPACES.replace_all(&decode(&no_tags), " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities() {
        assert_eq!(to_numeric("a&nbsp;b &amp; &inconnue;"), "a&#160;b &amp; &amp;inconnue;");
        assert_eq!(decode("&eacute;t&#233; &#x41; &lt;b&gt;"), "été A <b>");
    }

    #[test]
    fn strip() {
        assert_eq!(strip_html("<p>Un résumé <b>riche</b>.</p><p>Suite</p>"), "Un résumé riche. Suite");
    }
}
