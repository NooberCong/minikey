//! The Vietnamese lexicon: letters, vowel sequences, consonant sequences, and
//! the spelling rules that say how they may combine.

use Vs::*;

/// A letter without tone or case. Variants are named after their Telex
/// spelling: `Aa` = â, `Aw` = ă, `Dd` = đ, `Ee` = ê, `Oo` = ô, `Ow` = ơ, `Uw` = ư.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[rustfmt::skip]
pub enum Sym {
    A, Aa, Aw, B, C, D, Dd, E, Ee, F, G, H, I, J, K, L, M,
    N, O, Oo, Ow, P, Q, R, S, T, U, Uw, V, W, X, Y, Z,
    /// Anything that is not a letter (digits, symbols).
    NonVn,
}

impl Sym {
    pub const fn from_ascii(key: u8) -> Self {
        use Sym::*;
        const LETTERS: [Sym; 26] = [
            A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
        ];
        if key.is_ascii_alphabetic() {
            LETTERS[(key.to_ascii_lowercase() - b'a') as usize]
        } else {
            NonVn
        }
    }

    pub const fn is_vowel(self) -> bool {
        use Sym::*;
        matches!(self, A | Aa | Aw | E | Ee | I | O | Oo | Ow | U | Uw | Y)
    }

    /// The UTF-16 code unit for this letter, or `None` for `NonVn`. Every
    /// Vietnamese letter is in the BMP, so one letter is always one code unit
    /// and one on-screen character.
    pub const fn to_utf16(self, tone: u8, caps: bool) -> Option<u16> {
        use Sym::*;
        let row = match self {
            A => 0,
            Aa => 1,
            Aw => 2,
            E => 3,
            Ee => 4,
            I => 5,
            O => 6,
            Oo => 7,
            Ow => 8,
            U => 9,
            Uw => 10,
            Y => 11,
            Dd => return Some(if caps { 0x0110 } else { 0x0111 }),
            NonVn => return None,
            B => return Some(ascii(b'b', caps)),
            C => return Some(ascii(b'c', caps)),
            D => return Some(ascii(b'd', caps)),
            F => return Some(ascii(b'f', caps)),
            G => return Some(ascii(b'g', caps)),
            H => return Some(ascii(b'h', caps)),
            J => return Some(ascii(b'j', caps)),
            K => return Some(ascii(b'k', caps)),
            L => return Some(ascii(b'l', caps)),
            M => return Some(ascii(b'm', caps)),
            N => return Some(ascii(b'n', caps)),
            P => return Some(ascii(b'p', caps)),
            Q => return Some(ascii(b'q', caps)),
            R => return Some(ascii(b'r', caps)),
            S => return Some(ascii(b's', caps)),
            T => return Some(ascii(b't', caps)),
            V => return Some(ascii(b'v', caps)),
            W => return Some(ascii(b'w', caps)),
            X => return Some(ascii(b'x', caps)),
            Z => return Some(ascii(b'z', caps)),
        };
        Some(VOWEL_FORMS[caps as usize][row][tone as usize])
    }
}

const fn ascii(lower: u8, caps: bool) -> u16 {
    (if caps { lower.to_ascii_uppercase() } else { lower }) as u16
}

/// Precomposed vowels as `[case][vowel][tone]`, vowels in `to_utf16` row order,
/// tones as none, sắc, huyền, hỏi, ngã, nặng.
#[rustfmt::skip]
const VOWEL_FORMS: [[[u16; 6]; 12]; 2] = [[
    [0x0061, 0x00E1, 0x00E0, 0x1EA3, 0x00E3, 0x1EA1], // a
    [0x00E2, 0x1EA5, 0x1EA7, 0x1EA9, 0x1EAB, 0x1EAD], // â
    [0x0103, 0x1EAF, 0x1EB1, 0x1EB3, 0x1EB5, 0x1EB7], // ă
    [0x0065, 0x00E9, 0x00E8, 0x1EBB, 0x1EBD, 0x1EB9], // e
    [0x00EA, 0x1EBF, 0x1EC1, 0x1EC3, 0x1EC5, 0x1EC7], // ê
    [0x0069, 0x00ED, 0x00EC, 0x1EC9, 0x0129, 0x1ECB], // i
    [0x006F, 0x00F3, 0x00F2, 0x1ECF, 0x00F5, 0x1ECD], // o
    [0x00F4, 0x1ED1, 0x1ED3, 0x1ED5, 0x1ED7, 0x1ED9], // ô
    [0x01A1, 0x1EDB, 0x1EDD, 0x1EDF, 0x1EE1, 0x1EE3], // ơ
    [0x0075, 0x00FA, 0x00F9, 0x1EE7, 0x0169, 0x1EE5], // u
    [0x01B0, 0x1EE9, 0x1EEB, 0x1EED, 0x1EEF, 0x1EF1], // ư
    [0x0079, 0x00FD, 0x1EF3, 0x1EF7, 0x1EF9, 0x1EF5], // y
], [
    [0x0041, 0x00C1, 0x00C0, 0x1EA2, 0x00C3, 0x1EA0], // A
    [0x00C2, 0x1EA4, 0x1EA6, 0x1EA8, 0x1EAA, 0x1EAC], // Â
    [0x0102, 0x1EAE, 0x1EB0, 0x1EB2, 0x1EB4, 0x1EB6], // Ă
    [0x0045, 0x00C9, 0x00C8, 0x1EBA, 0x1EBC, 0x1EB8], // E
    [0x00CA, 0x1EBE, 0x1EC0, 0x1EC2, 0x1EC4, 0x1EC6], // Ê
    [0x0049, 0x00CD, 0x00CC, 0x1EC8, 0x0128, 0x1ECA], // I
    [0x004F, 0x00D3, 0x00D2, 0x1ECE, 0x00D5, 0x1ECC], // O
    [0x00D4, 0x1ED0, 0x1ED2, 0x1ED4, 0x1ED6, 0x1ED8], // Ô
    [0x01A0, 0x1EDA, 0x1EDC, 0x1EDE, 0x1EE0, 0x1EE2], // Ơ
    [0x0055, 0x00DA, 0x00D9, 0x1EE6, 0x0168, 0x1EE4], // U
    [0x01AF, 0x1EE8, 0x1EEA, 0x1EEC, 0x1EEE, 0x1EF0], // Ư
    [0x0059, 0x00DD, 0x1EF2, 0x1EF6, 0x1EF8, 0x1EF4], // Y
]];

/// A consonant cluster that can open or close a syllable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[rustfmt::skip]
pub enum Cs {
    Nil,
    B, C, Ch, D, Dd, Dz, G, Gh, Gi, Gin, K, Kh, L, M, N, Ng, Ngh, Nh,
    P, Ph, Q, Qu, R, S, T, Th, Tr, V, X,
}

const CLUSTERS: [(Cs, [Sym; 3]); 29] = {
    use Sym::NonVn as __;
    [
        (Cs::B, [Sym::B, __, __]),
        (Cs::C, [Sym::C, __, __]),
        (Cs::Ch, [Sym::C, Sym::H, __]),
        (Cs::D, [Sym::D, __, __]),
        (Cs::Dd, [Sym::Dd, __, __]),
        (Cs::Dz, [Sym::D, Sym::Z, __]),
        (Cs::G, [Sym::G, __, __]),
        (Cs::Gh, [Sym::G, Sym::H, __]),
        (Cs::Gi, [Sym::G, Sym::I, __]),
        (Cs::Gin, [Sym::G, Sym::I, Sym::N]),
        (Cs::K, [Sym::K, __, __]),
        (Cs::Kh, [Sym::K, Sym::H, __]),
        (Cs::L, [Sym::L, __, __]),
        (Cs::M, [Sym::M, __, __]),
        (Cs::N, [Sym::N, __, __]),
        (Cs::Ng, [Sym::N, Sym::G, __]),
        (Cs::Ngh, [Sym::N, Sym::G, Sym::H]),
        (Cs::Nh, [Sym::N, Sym::H, __]),
        (Cs::P, [Sym::P, __, __]),
        (Cs::Ph, [Sym::P, Sym::H, __]),
        (Cs::Q, [Sym::Q, __, __]),
        (Cs::Qu, [Sym::Q, Sym::U, __]),
        (Cs::R, [Sym::R, __, __]),
        (Cs::S, [Sym::S, __, __]),
        (Cs::T, [Sym::T, __, __]),
        (Cs::Th, [Sym::T, Sym::H, __]),
        (Cs::Tr, [Sym::T, Sym::R, __]),
        (Cs::V, [Sym::V, __, __]),
        (Cs::X, [Sym::X, __, __]),
    ]
};

impl Cs {
    pub fn lookup(letters: [Sym; 3]) -> Self {
        CLUSTERS
            .iter()
            .find(|(_, spelling)| *spelling == letters)
            .map_or(Cs::Nil, |(cluster, _)| *cluster)
    }

    /// The letters of this cluster and how many there are. `Nil` reads as the
    /// first cluster, a lone consonant.
    pub fn letters(self) -> ([Sym; 3], usize) {
        let (_, spelling) = CLUSTERS
            .iter()
            .find(|(cluster, _)| *cluster == self)
            .unwrap_or(&CLUSTERS[0]);
        let len = spelling.iter().take_while(|s| **s != Sym::NonVn).count();
        (*spelling, len)
    }

    /// The bit for this cluster in [`VowelSeq::codas`], or 0 if it cannot end
    /// a syllable.
    const fn coda_bit(self) -> u8 {
        match self {
            Cs::C => C,
            Cs::Ch => CH,
            Cs::M => M,
            Cs::N => N,
            Cs::Ng => NG,
            Cs::Nh => NH,
            Cs::P => P,
            Cs::T => T,
            _ => 0,
        }
    }
}

const C: u8 = 1 << 0;
const CH: u8 = 1 << 1;
const M: u8 = 1 << 2;
const N: u8 = 1 << 3;
const NG: u8 = 1 << 4;
const NH: u8 = 1 << 5;
const P: u8 = 1 << 6;
const T: u8 = 1 << 7;

/// A vowel sequence, named letter by letter (`UwOw` = ươ).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[rustfmt::skip]
pub enum Vs {
    A, Aa, Aw, E, Ee, I, O, Oo, Ow, U, Uw, Y,
    AI, AO, AU, AY, AaU, AaY, EO, EU, EeU, IA, IE, IEe, IU,
    OA, OAw, OE, OI, OoI, OwI, UA, UAa, UE, UEe, UI, UO, UOo, UOw,
    UU, UY, UwA, UwI, UwO, UwOw, UwU, YE, YEe,
    IEU, IEeU, OAI, OAY, OEO, UAY, UAaY, UOI, UOU, UOoI, UOwI, UOwU,
    UYA, UYE, UYEe, UYU, UwOI, UwOU, UwOwI, UwOwU, YEU, YEeU,
    Nil,
}

/// Spelling facts about one vowel sequence.
pub struct VowelSeq {
    id: Vs,
    pub letters: [Sym; 3],
    pub len: i32,
    /// Position of the letter carrying ^, or -1.
    pub roof_pos: i32,
    /// The sequence after adding ^.
    pub with_roof: Vs,
    /// Position of the letter carrying a horn or breve, or -1.
    pub hook_pos: i32,
    /// The sequence after adding a horn or breve.
    pub with_hook: Vs,
    /// Which final consonants may follow, as a bit set.
    codas: u8,
}

const fn row(
    id: Vs,
    spelling: &[Sym],
    roof_pos: i32,
    with_roof: Vs,
    hook_pos: i32,
    with_hook: Vs,
    codas: u8,
) -> VowelSeq {
    let mut letters = [Sym::NonVn; 3];
    let mut i = 0;
    while i < spelling.len() {
        letters[i] = spelling[i];
        i += 1;
    }
    VowelSeq { id, letters, len: spelling.len() as i32, roof_pos, with_roof, hook_pos, with_hook, codas }
}

/// Indexed by `Vs`; `vowel_table_is_indexed_by_id` guards the order.
#[rustfmt::skip]
static VOWEL_SEQS: [VowelSeq; 70] = {
    use Sym::{A as a, Aa as aa, Aw as aw, E as e, Ee as ee, I as i, O as o, Oo as oo, Ow as ow, U as u, Uw as uw, Y as y};
    [
        row(A, &[a], -1, Aa, -1, Aw, C | CH | M | N | NG | NH | P | T),
        row(Aa, &[aa], 0, Nil, -1, Aw, C | M | N | NG | P | T),
        row(Aw, &[aw], -1, Aa, 0, Nil, C | M | N | NG | P | T),
        row(E, &[e], -1, Ee, -1, Nil, C | CH | M | N | NG | NH | P | T),
        row(Ee, &[ee], 0, Nil, -1, Nil, C | CH | M | N | NH | P | T),
        row(I, &[i], -1, Nil, -1, Nil, C | CH | M | N | NH | P | T),
        row(O, &[o], -1, Oo, -1, Ow, C | M | N | NG | P | T),
        row(Oo, &[oo], 0, Nil, -1, Ow, C | M | N | NG | P | T),
        row(Ow, &[ow], -1, Oo, 0, Nil, M | N | P | T),
        row(U, &[u], -1, Nil, -1, Uw, C | M | N | NG | P | T),
        row(Uw, &[uw], -1, Nil, 0, Nil, C | M | N | NG | T),
        row(Y, &[y], -1, Nil, -1, Nil, T),
        row(AI, &[a, i], -1, Nil, -1, Nil, 0),
        row(AO, &[a, o], -1, Nil, -1, Nil, 0),
        row(AU, &[a, u], -1, AaU, -1, Nil, 0),
        row(AY, &[a, y], -1, AaY, -1, Nil, 0),
        row(AaU, &[aa, u], 0, Nil, -1, Nil, 0),
        row(AaY, &[aa, y], 0, Nil, -1, Nil, 0),
        row(EO, &[e, o], -1, Nil, -1, Nil, 0),
        row(EU, &[e, u], -1, EeU, -1, Nil, 0),
        row(EeU, &[ee, u], 0, Nil, -1, Nil, 0),
        row(IA, &[i, a], -1, Nil, -1, Nil, 0),
        row(IE, &[i, e], -1, IEe, -1, Nil, C | M | N | NG | P | T),
        row(IEe, &[i, ee], 1, Nil, -1, Nil, C | M | N | NG | P | T),
        row(IU, &[i, u], -1, Nil, -1, Nil, 0),
        row(OA, &[o, a], -1, Nil, -1, OAw, C | CH | M | N | NG | NH | P | T),
        row(OAw, &[o, aw], -1, Nil, 1, Nil, C | M | N | NG | T),
        row(OE, &[o, e], -1, Nil, -1, Nil, N | T),
        row(OI, &[o, i], -1, OoI, -1, OwI, 0),
        row(OoI, &[oo, i], 0, Nil, -1, OwI, 0),
        row(OwI, &[ow, i], -1, OoI, 0, Nil, 0),
        row(UA, &[u, a], -1, UAa, -1, UwA, N | NG | T),
        row(UAa, &[u, aa], 1, Nil, -1, Nil, N | NG | T),
        row(UE, &[u, e], -1, UEe, -1, Nil, C | CH | N | NH),
        row(UEe, &[u, ee], 1, Nil, -1, Nil, C | CH | N | NH),
        row(UI, &[u, i], -1, Nil, -1, UwI, 0),
        row(UO, &[u, o], -1, UOo, -1, UwO, C | M | N | NG | P | T),
        row(UOo, &[u, oo], 1, Nil, -1, UOw, C | M | N | NG | T),
        row(UOw, &[u, ow], -1, UOo, 1, UwOw, 0),
        row(UU, &[u, u], -1, Nil, -1, UwU, 0),
        row(UY, &[u, y], -1, Nil, -1, Nil, C | CH | N | NH | P | T),
        row(UwA, &[uw, a], -1, Nil, 0, Nil, 0),
        row(UwI, &[uw, i], -1, Nil, 0, Nil, 0),
        row(UwO, &[uw, o], -1, Nil, 0, UwOw, C | M | N | NG | P | T),
        row(UwOw, &[uw, ow], -1, Nil, 0, Nil, C | M | N | NG | P | T),
        row(UwU, &[uw, u], -1, Nil, 0, Nil, 0),
        row(YE, &[y, e], -1, YEe, -1, Nil, M | N | NG | P | T),
        row(YEe, &[y, ee], 1, Nil, -1, Nil, M | N | NG | T),
        row(IEU, &[i, e, u], -1, IEeU, -1, Nil, 0),
        row(IEeU, &[i, ee, u], 1, Nil, -1, Nil, 0),
        row(OAI, &[o, a, i], -1, Nil, -1, Nil, 0),
        row(OAY, &[o, a, y], -1, Nil, -1, Nil, 0),
        row(OEO, &[o, e, o], -1, Nil, -1, Nil, 0),
        row(UAY, &[u, a, y], -1, UAaY, -1, Nil, 0),
        row(UAaY, &[u, aa, y], 1, Nil, -1, Nil, 0),
        row(UOI, &[u, o, i], -1, UOoI, -1, UwOI, 0),
        row(UOU, &[u, o, u], -1, Nil, -1, UwOU, 0),
        row(UOoI, &[u, oo, i], 1, Nil, -1, UOwI, 0),
        row(UOwI, &[u, ow, i], -1, UOoI, 1, UwOwI, 0),
        row(UOwU, &[u, ow, u], -1, Nil, 1, UwOwU, 0),
        row(UYA, &[u, y, a], -1, Nil, -1, Nil, 0),
        row(UYE, &[u, y, e], -1, UYEe, -1, Nil, N | T),
        row(UYEe, &[u, y, ee], 2, Nil, -1, Nil, N | T),
        row(UYU, &[u, y, u], -1, Nil, -1, Nil, 0),
        row(UwOI, &[uw, o, i], -1, Nil, 0, UwOwI, 0),
        row(UwOU, &[uw, o, u], -1, Nil, 0, UwOwU, 0),
        row(UwOwI, &[uw, ow, i], -1, Nil, 0, Nil, 0),
        row(UwOwU, &[uw, ow, u], -1, Nil, 0, Nil, 0),
        row(YEU, &[y, e, u], -1, YEeU, -1, Nil, 0),
        row(YEeU, &[y, ee, u], 1, Nil, -1, Nil, 0),
    ]
};

impl Vs {
    /// `Nil` reads as the first row, a plain one-letter sequence.
    pub fn info(self) -> &'static VowelSeq {
        VOWEL_SEQS.get(self as usize).unwrap_or(&VOWEL_SEQS[0])
    }

    pub fn lookup(letters: [Sym; 3]) -> Self {
        VOWEL_SEQS
            .iter()
            .find(|seq| seq.letters == letters)
            .map_or(Nil, |seq| seq.id)
    }

    pub fn lookup1(letter: Sym) -> Self {
        Self::lookup([letter, Sym::NonVn, Sym::NonVn])
    }

    /// The sequence formed by the first `len` letters of this one.
    pub fn prefix(self, len: i32) -> Self {
        let mut letters = self.info().letters;
        for letter in letters.iter_mut().skip(len as usize) {
            *letter = Sym::NonVn;
        }
        Self::lookup(letters)
    }
}

/// Can vowel sequence `vs` be followed by final consonant `cs`?
fn is_valid_vc(vs: Vs, cs: Cs) -> bool {
    vs == Nil || cs == Cs::Nil || vs.info().codas & cs.coda_bit() != 0
}

/// Can initial consonant `cs` be followed by vowel sequence `vs`?
pub fn is_valid_cv(cs: Cs, vs: Vs) -> bool {
    if cs == Cs::Nil || vs == Nil {
        return true;
    }
    let first = vs.info().letters[0];
    match cs {
        Cs::Gi => first != Sym::I,
        Cs::Qu => first != Sym::U,
        Cs::K => matches!(vs, E | I | Y | Ee | EO | EU | EeU | IA | IE | IEe | IEU | IEeU),
        _ => true,
    }
}

/// Is initial + vowels + final a spellable syllable?
pub fn is_valid_cvc(c1: Cs, vs: Vs, c2: Cs) -> bool {
    if vs == Nil {
        return c1 == Cs::Nil || c2 != Cs::Nil;
    }
    if c1 == Cs::Nil {
        return is_valid_vc(vs, c2);
    }
    if c2 == Cs::Nil {
        return is_valid_cv(c1, vs);
    }
    if is_valid_vc(vs, c2) {
        return is_valid_cv(c1, vs);
    }
    // Spellings that only work because the initial absorbs a vowel.
    let quynh = c1 == Cs::Qu && vs == Y && matches!(c2, Cs::N | Cs::Nh);
    let gieng = c1 == Cs::Gi && matches!(vs, E | Ee) && matches!(c2, Cs::N | Cs::Ng);
    quynh || gieng
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vowel_table_is_indexed_by_id() {
        for (index, seq) in VOWEL_SEQS.iter().enumerate() {
            assert_eq!(seq.id as usize, index);
        }
        assert_eq!(Nil as usize, VOWEL_SEQS.len());
    }

    #[test]
    fn every_prefix_of_a_vowel_sequence_is_a_sequence() {
        for seq in &VOWEL_SEQS {
            for len in 1..seq.len {
                assert_ne!(seq.id.prefix(len), Nil, "{:?} prefix {len}", seq.id);
            }
        }
    }

    #[test]
    fn upper_and_lower_vowel_forms_are_case_pairs() {
        for (lower, upper) in VOWEL_FORMS[0].iter().zip(&VOWEL_FORMS[1]) {
            for (l, u) in lower.iter().zip(upper) {
                let l = char::from_u32(u32::from(*l)).unwrap();
                let u = char::from_u32(u32::from(*u)).unwrap();
                assert_eq!(l.to_uppercase().next(), Some(u));
            }
        }
    }
}
