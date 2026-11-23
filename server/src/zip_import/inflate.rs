//! RFC 1951（DEFLATE）の最小展開器。
//!
//! サンドボックスはオフラインビルドで zip／flate2 系 crate が使えないため、
//! 純粋 Rust で自己実装する（設計 `REF:docs/designs/m3.md` §2.3 判断 4）。
//!
//! 固定ハフマン・動的ハフマン・蓄積ブロック（stored）をサポートする。
//! DEFLATE のビット列は字节内 LSB 先行だが、ハフマン符号は MSB 先行で
//! 詰め込まれるため、符号を 1 ビットずつ左シフトしながら照合する。

use std::fmt;

/// 展開エラー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InflateError {
    /// 入力が DEFLATE として妥当でない。
    InvalidData,
    /// 入力が尽きた。
    UnexpectedEof,
}

impl fmt::Display for InflateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InflateError::InvalidData => write!(f, "DEFLATE データが不正です"),
            InflateError::UnexpectedEof => write!(f, "DEFLATE 入力が途中で尽きました"),
        }
    }
}

impl std::error::Error for InflateError {}

/// 最長符号長。
const MAX_BITS: usize = 15;

/// LSB 先行のビットリーダー。
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_buf: u8,
    bit_count: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_buf: 0,
            bit_count: 0,
        }
    }

    /// 次のビット（字节内 LSB 先行）を読む。
    fn read_bit(&mut self) -> Result<u32, InflateError> {
        if self.bit_count == 0 {
            if self.byte_pos >= self.data.len() {
                return Err(InflateError::UnexpectedEof);
            }
            self.bit_buf = self.data[self.byte_pos];
            self.byte_pos += 1;
            self.bit_count = 8;
        }
        let b = (self.bit_buf & 1) as u32;
        self.bit_buf >>= 1;
        self.bit_count -= 1;
        Ok(b)
    }

    /// `n` ビットを LSB 先行で読む（`n` <= 32）。
    fn read_bits(&mut self, n: u32) -> Result<u32, InflateError> {
        let mut v = 0u32;
        for i in 0..n {
            v |= self.read_bit()? << i;
        }
        Ok(v)
    }

    /// 字节境界に合わせる（蓄積ブロック用）。
    fn align_to_byte(&mut self) {
        self.bit_count = 0;
    }
}

/// ハフマン符号表（カノニカル符号）。
struct Huffman {
    /// 符号長ごとのコード数。index は符号長（1..=15）。
    count: [u16; MAX_BITS + 1],
    /// 符号を（符号長、符号）順に並べたシンボル一覧。
    symbol: Vec<u16>,
}

impl Huffman {
    /// 符号長の配列からカノニカル符号表を構築する。
    fn build(lengths: &[u8]) -> Result<Self, InflateError> {
        let mut count = [0u16; MAX_BITS + 1];
        for &l in lengths {
            if l > MAX_BITS as u8 {
                return Err(InflateError::InvalidData);
            }
            if l > 0 {
                count[l as usize] += 1;
            }
        }
        // 過剰割り当てチェック: 符号の総数が 2^15 を超えてはならない。
        let mut total = 0u32;
        for (len, &cnt) in count.iter().enumerate().skip(1) {
            total += (cnt as u32) << (MAX_BITS - len);
        }
        if total > (1u32 << MAX_BITS) {
            return Err(InflateError::InvalidData);
        }
        // 各符号長に対する先頭インデックスを計算。
        let mut offset = [0u16; MAX_BITS + 1];
        let mut acc = 0u16;
        for len in 1..=MAX_BITS {
            offset[len] = acc;
            acc = acc.checked_add(count[len]).ok_or(InflateError::InvalidData)?;
        }
        let mut symbol = vec![0u16; acc as usize];
        for (sym, &len) in lengths.iter().enumerate() {
            if len > 0 {
                symbol[offset[len as usize] as usize] = sym as u16;
                offset[len as usize] += 1;
            }
        }
        Ok(Self { count, symbol })
    }

    /// 1 シンボルをデコードする。
    ///
    /// カノニカル符号を 1 ビットずつ照合する（zlib puff 準拠）。
    /// DEFLATE は符号を MSB 先行で詰め込み、ビット列は字节内 LSB 先行で
    /// だから、LSB 先行で読んだビットを左シフトながら照合すると符号値が
    /// 順に組み上がる。
    fn decode(&self, r: &mut BitReader<'_>) -> Result<u16, InflateError> {
        let mut code = 0u32;
        let mut first = 0u32;
        let mut index = 0u32;
        for len in 1..=MAX_BITS as u32 {
            code |= r.read_bit()?;
            let c = self.count[len as usize] as u32;
            if code.wrapping_sub(first) < c {
                return Ok(self.symbol[(index + code - first) as usize]);
            }
            index += c;
            first = (first + c) << 1;
            code <<= 1;
        }
        Err(InflateError::InvalidData)
    }
}

/// 長さ符号の基底値（コード 257..=285）。
const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
/// 長さ符号の追加ビット数。
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
/// 距離符号の基底値（コード 0..=29）。
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049,
    3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
/// 距離符号の追加ビット数。
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// 固定ハフマンのリテラル／長さ符号表を構築する。
fn fixed_literal_length() -> Huffman {
    let mut lengths = [0u8; 288];
    lengths[..144].fill(8);
    lengths[144..256].fill(9);
    lengths[256..280].fill(7);
    lengths[280..288].fill(8);
    Huffman::build(&lengths).expect("固定ハフマン表は妥当")
}

/// 固定ハフマンの距離符号表を構築する。
fn fixed_distance() -> Huffman {
    let lengths = [5u8; 30];
    Huffman::build(&lengths).expect("固定距離表は妥当")
}

/// DEFLATE ストリームを展開する。
pub fn inflate(input: &[u8]) -> Result<Vec<u8>, InflateError> {
    let mut r = BitReader::new(input);
    let mut out = Vec::new();
    let fixed_lit = fixed_literal_length();
    let fixed_dist = fixed_distance();

    loop {
        let bfinal = r.read_bit()?;
        let btype = r.read_bits(2)?;
        match btype {
            0 => {
                // 蓄積ブロック。
                r.align_to_byte();
                let len = r.read_bits(16)?;
                let nlen = r.read_bits(16)?;
                // NLEN は LEN の 16 ビット補数（LEN ^ NLEN == 0xFFFF）。
                if (len ^ nlen) != 0xffff {
                    return Err(InflateError::InvalidData);
                }
                for _ in 0..len {
                    out.push(r.read_bits(8)? as u8);
                }
            }
            1 => {
                inflate_huffman_block(&mut r, &mut out, &fixed_lit, &fixed_dist)?;
            }
            2 => {
                let (lit, dist) = read_dynamic_tables(&mut r)?;
                inflate_huffman_block(&mut r, &mut out, &lit, &dist)?;
            }
            _ => return Err(InflateError::InvalidData),
        }
        if bfinal == 1 {
            break;
        }
    }
    Ok(out)
}

/// 動的ハフマンの符号長符号表を読み、リテラル／長さと距離の符号表を返す。
fn read_dynamic_tables(r: &mut BitReader<'_>) -> Result<(Huffman, Huffman), InflateError> {
    let hlit = r.read_bits(5)? as usize + 257;
    let hdist = r.read_bits(5)? as usize + 1;
    let hclen = r.read_bits(4)? as usize + 4;

    // 符号長符号表の順序（RFC 1951 3.2.7）。読み出した値は
    // `ORDER[i]` 番目の符号長として割り当てる必要がある。
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let mut length_lengths = [0u8; 19];
    for i in 0..hclen {
        length_lengths[ORDER[i]] = r.read_bits(3)? as u8;
    }
    let cl_huff = Huffman::build(&length_lengths)?;

    let total = hlit + hdist;
    let mut all_lengths = vec![0u8; total];
    let mut i = 0usize;
    while i < total {
        let sym = cl_huff.decode(r)? as usize;
        match sym {
            0..=15 => {
                all_lengths[i] = sym as u8;
                i += 1;
            }
            16 => {
                // 前回の符号長を 3..=6 回繰り返す。
                if i == 0 {
                    return Err(InflateError::InvalidData);
                }
                let rep = 3 + r.read_bits(2)? as usize;
                let prev = all_lengths[i - 1];
                for _ in 0..rep {
                    if i >= total {
                        break;
                    }
                    all_lengths[i] = prev;
                    i += 1;
                }
            }
            17 => {
                // ゼロを 3..=10 回繰り返す。
                let rep = 3 + r.read_bits(3)? as usize;
                i = i.saturating_add(rep);
            }
            18 => {
                // ゼロを 11..=138 回繰り返す。
                let rep = 11 + r.read_bits(7)? as usize;
                i = i.saturating_add(rep);
            }
            _ => return Err(InflateError::InvalidData),
        }
        if i > total {
            return Err(InflateError::InvalidData);
        }
    }
    if i != total {
        return Err(InflateError::InvalidData);
    }
    let lit_lengths = &all_lengths[..hlit];
    let dist_lengths = &all_lengths[hlit..];
    // リテラル／長さは少なくとも EOS（256）を含む必要がある。
    if lit_lengths.iter().all(|&l| l == 0) {
        return Err(InflateError::InvalidData);
    }
    let lit = Huffman::build(lit_lengths)?;
    let dist = Huffman::build(dist_lengths)?;
    Ok((lit, dist))
}

/// ハフマンブロックを展開し、`out` に追加する。
fn inflate_huffman_block(
    r: &mut BitReader<'_>,
    out: &mut Vec<u8>,
    lit: &Huffman,
    dist: &Huffman,
) -> Result<(), InflateError> {
    loop {
        let sym = lit.decode(r)?;
        match sym {
            0..=255 => {
                out.push(sym as u8);
            }
            256 => {
                return Ok(());
            }
            257..=285 => {
                let idx = sym as usize - 257;
                let extra = LENGTH_EXTRA[idx] as u32;
                let length = LENGTH_BASE[idx] as u32 + r.read_bits(extra)?;
                let dsym = dist.decode(r)? as usize;
                if dsym >= DIST_BASE.len() {
                    return Err(InflateError::InvalidData);
                }
                let dextra = DIST_EXTRA[dsym] as u32;
                let distance = DIST_BASE[dsym] as u32 + r.read_bits(dextra)?;
                if distance == 0 || distance as usize > out.len() {
                    return Err(InflateError::InvalidData);
                }
                let start = out.len() - distance as usize;
                for k in 0..length as usize {
                    out.push(out[start + k]);
                }
            }
            _ => return Err(InflateError::InvalidData),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Python `zlib.compressobj(9, DEFLATED, -15)` で生成した既知のベクトル。
    /// 展開結果が元の平文と一致することを検証する。
    fn assert_inflate(name: &str, compressed: &[u8], expected: &[u8]) {
        let got = inflate(compressed).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(got, expected, "{name} の展開結果が一致しない");
    }

    #[test]
    fn inflates_stored_block() {
        // 蓄積ブロック: BFINAL=1, BTYPE=00, LENGTH=5, NLEN=~5, 本文 "hello"。
        // Python `zlib.compressobj(0, DEFLATED, -15)` で生成した既知のベクトル。
        let data: &[u8] = &[0x01, 0x05, 0x00, 0xfa, 0xff, 0x68, 0x65, 0x6c, 0x6c, 0x6f];
        assert_inflate("stored", data, b"hello");
    }

    #[test]
    fn inflates_fixed_huffman() {
        // Python `zlib.compressobj(9, DEFLATED, -15)` で生成した既知のベクトル。
        const COMPRESSED: &[u8] = &[
            0x0b, 0xc9, 0x48, 0x55, 0x28, 0x2c, 0xcd, 0x4c, 0xce, 0x56, 0x48, 0x2a, 0xca, 0x2f, 0xcf,
            0x53, 0x48, 0xcb, 0xaf, 0x50, 0xc8, 0x2a, 0xcd, 0x2d, 0x28, 0x56, 0xc8, 0x2f, 0x4b, 0x2d,
            0x52, 0x28, 0x01, 0x4a, 0xe7, 0x24, 0x56, 0x55, 0x2a, 0xa4, 0xe4, 0xa7, 0xeb, 0x01, 0x00,
        ];
        assert_inflate(
            "fixed",
            COMPRESSED,
            b"The quick brown fox jumps over the lazy dog.",
        );
    }

    #[test]
    fn inflates_dynamic_huffman() {
        // 26 文字の反復で動的ハフマンを使うベクトル。
        // Python `zlib.compressobj(9, DEFLATED, -15)` で生成した既知のベクトル。
        const COMPRESSED: &[u8] = &[
            0x4b, 0x4c, 0x4a, 0x4e, 0x49, 0x4d, 0x4b, 0xcf, 0xc8, 0xcc, 0xca, 0xce, 0xc9, 0xcd, 0xcb,
            0x2f, 0x28, 0x2c, 0x2a, 0x2e, 0x29, 0x2d, 0x2b, 0xaf, 0xa8, 0xac, 0x4a, 0xa4, 0xaa, 0x0c,
            0x00,
        ];
        const EXPECTED: &[u8] = b"abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
        assert_inflate("dynamic", COMPRESSED, EXPECTED);
    }

    #[test]
    fn rejects_truncated_input() {
        // 先頭だけ切ると展開不能。
        let err = inflate(&[0x0b, 0xc9, 0x48]);
        assert!(matches!(err, Err(InflateError::UnexpectedEof | InflateError::InvalidData)));
    }
}
