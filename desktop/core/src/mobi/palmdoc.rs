//! Compression PalmDoc (LZ77 simplifié) des enregistrements texte MOBI.

pub fn decompress(input: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(input.len() * 2);
    let mut i = 0;
    while i < input.len() {
        let c = input[i] as usize;
        i += 1;
        if (1..=8).contains(&c) {
            let end = (i + c).min(input.len());
            out.extend_from_slice(&input[i..end]);
            i = end;
        } else if c < 0x80 {
            out.push(c as u8);
        } else if c >= 0xC0 {
            out.push(0x20);
            out.push((c ^ 0x80) as u8);
        } else {
            if i >= input.len() {
                break;
            }
            let pair = (c << 8) | input[i] as usize;
            i += 1;
            let distance = (pair >> 3) & 0x07FF;
            let length = (pair & 7) + 3;
            if distance == 0 || distance > out.len() {
                continue;
            }
            let start = out.len() - distance;
            for k in 0..length {
                let b = out[start + k];
                out.push(b);
            }
        }
    }
    out
}

/// Compression équivalente à celle de calibre (cPalmdoc).
pub fn compress(data: &[u8]) -> Vec<u8> {
    let n = data.len();
    let mut out = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        if i > 10 && n - i > 10
            && let Some((j, length)) = longest_match(data, i) {
                let compound = ((i - j) << 3) + length - 3;
                out.push((0x80 + (compound >> 8)) as u8);
                out.push((compound & 0xFF) as u8);
                i += length;
                continue;
            }
        let mut c = data[i];
        i += 1;
        if c == 0x20 && i < n {
            let next = data[i];
            if (0x40..=0x7F).contains(&next) {
                out.push(next ^ 0x80);
                i += 1;
                continue;
            }
        }
        if c == 0 || (c > 8 && c < 0x80) {
            out.push(c);
        } else {
            // Suite d'octets « binaires » échappés par un préfixe de longueur 1 à 8.
            let mut run = vec![c];
            let mut j = i;
            while j < n && run.len() < 8 {
                c = data[j];
                if c == 0 || (c > 8 && c < 0x80) {
                    break;
                }
                run.push(c);
                j += 1;
            }
            i += run.len() - 1;
            out.push(run.len() as u8);
            out.extend_from_slice(&run);
        }
    }
    out
}

/// Correspondance la plus longue (3 à 10 octets) dans la fenêtre de 2047 octets ; à longueur égale, la plus proche.
fn longest_match(data: &[u8], pos: usize) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let limit = pos.saturating_sub(2047);
    let first = data[pos];
    let mut j = pos - 3;
    loop {
        if data[j] == first {
            // La source ne doit pas chevaucher la position courante.
            let max_length = 10.min(pos - j);
            let mut k = 1;
            while k < max_length && data[j + k] == data[pos + k] {
                k += 1;
            }
            if k >= 3 && k > best.map_or(0, |b| b.1) {
                best = Some((j, k));
                if k == 10 {
                    break;
                }
            }
        }
        if j == limit {
            break;
        }
        j -= 1;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let text = "Le chat dort. Le chien aboie. ÀÉÎ — ok\u{1}\u{2}\n".repeat(120).into_bytes();
        for block in text.chunks(4096) {
            let packed = compress(block);
            assert_eq!(decompress(&packed), block);
            assert!(packed.len() < block.len());
        }
    }
}
