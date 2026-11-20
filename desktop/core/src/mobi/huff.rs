//! Décompression HUFF/CDIC (portage du lecteur de KindleUnpack, comme `HuffCDICReader`).

use crate::error::{BookError, Result};
use crate::util::bytes::Bytes;

pub struct HuffCdicReader {
    dict1: Vec<(usize, bool, u64)>,
    mincode: Vec<u64>,
    maxcode: Vec<u64>,
    dictionary: Vec<Option<(Vec<u8>, bool)>>,
}

impl HuffCdicReader {
    pub fn new(huff: &[u8], cdics: &[&[u8]]) -> Result<Self> {
        if !huff.starts_with_str("HUFF") || huff.u32_at(4) != 0x18 {
            return Err(BookError::Unreadable("enregistrement HUFF invalide".into()));
        }
        let (off1, off2) = (huff.u32_at(8), huff.u32_at(12));
        let mut dict1 = Vec::with_capacity(256);
        for i in 0..256 {
            let v = huff.u32_at(off1 + 4 * i) as u64;
            let codelen = (v & 0x1F) as usize;
            let term = v & 0x80 != 0;
            let mc = v >> 8;
            let max = if codelen == 0 { 0 } else { ((mc + 1) << (32 - codelen as u64)).wrapping_sub(1) };
            dict1.push((codelen, term, max));
        }
        let mut mincode = vec![0u64];
        let mut maxcode = vec![0xFFFF_FFFFu64];
        for codelen in 1..=32u64 {
            let lo = huff.u32_at(off2 + 8 * (codelen as usize - 1)) as u64;
            let hi = huff.u32_at(off2 + 8 * (codelen as usize - 1) + 4) as u64;
            mincode.push(lo << (32 - codelen));
            maxcode.push(((hi + 1) << (32 - codelen)).wrapping_sub(1));
        }
        let mut dictionary = Vec::new();
        for cdic in cdics {
            if !cdic.starts_with_str("CDIC") || cdic.u32_at(4) != 0x10 {
                return Err(BookError::Unreadable("enregistrement CDIC invalide".into()));
            }
            let (phrases, bits) = (cdic.u32_at(8), cdic.u32_at(12));
            let n = (1usize << bits.min(31)).min(phrases.saturating_sub(dictionary.len()));
            for k in 0..n {
                let off = cdic.u16_at(16 + 2 * k);
                let blen = cdic.u16_at(16 + off);
                dictionary.push(Some((cdic.slice_at(18 + off, blen & 0x7FFF).to_vec(), blen & 0x8000 != 0)));
            }
        }
        Ok(HuffCdicReader { dict1, mincode, maxcode, dictionary })
    }

    pub fn unpack(&mut self, input: &[u8]) -> Vec<u8> {
        let mut bits_left = input.len() as i64 * 8;
        let mut data = input.to_vec();
        data.extend_from_slice(&[0; 8]);
        let q = |pos: usize| -> u64 { (0..8).fold(0u64, |v, k| (v << 8) | data[pos + k] as u64) };
        let mut pos = 0;
        let mut x = q(pos);
        let mut n: i64 = 32;
        let mut out = Vec::new();
        loop {
            if n <= 0 {
                pos += 4;
                if pos + 8 > data.len() {
                    break;
                }
                x = q(pos);
                n += 32;
            }
            let code = (x >> n as u64) & 0xFFFF_FFFF;
            let (mut codelen, term, mut mc) = self.dict1[(code >> 24) as usize];
            if !term {
                while codelen < 32 && code < self.mincode[codelen] {
                    codelen += 1;
                }
                mc = self.maxcode[codelen];
            }
            n -= codelen as i64;
            bits_left -= codelen as i64;
            if bits_left < 0 || codelen == 0 {
                break;
            }
            let r = ((mc.wrapping_sub(code)) >> (32 - codelen as u64)) as usize;
            let Some(Some((bytes, done))) = self.dictionary.get(r).cloned() else { break };
            if done {
                out.extend_from_slice(&bytes);
            } else {
                self.dictionary[r] = None;
                let expanded = self.unpack(&bytes);
                out.extend_from_slice(&expanded);
                self.dictionary[r] = Some((expanded, true));
            }
        }
        out
    }
}
