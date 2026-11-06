//! Types through the engine the way the keyboard hook does, against a model of
//! a text field, and checks what ends up in it.

use minikey::{Engine, Method};

#[derive(Clone, Copy)]
enum Key {
    Char(u8),
    Space,
    Backspace,
    Reset,
}

fn press(engine: &mut Engine, screen: &mut Vec<u16>, key: Key) {
    let edit = match key {
        Key::Char(c) => {
            let edit = engine.key(c);
            if edit.is_none() {
                screen.push(u16::from(c));
            }
            edit
        }
        Key::Space => {
            engine.end_word();
            screen.push(u16::from(b' '));
            None
        }
        Key::Backspace => {
            let edit = engine.backspace();
            if edit.is_none() {
                screen.pop();
            }
            edit
        }
        Key::Reset => {
            engine.reset();
            None
        }
    };
    if let Some(edit) = edit {
        assert!(edit.backspaces <= screen.len(), "edit deletes more than is on screen");
        screen.truncate(screen.len() - edit.backspaces);
        screen.extend_from_slice(edit.text());
    }
}

/// `<` in `keys` stands for backspace, as nothing here needs a literal one.
fn typed(method: Method, keys: &str) -> String {
    let mut engine = Engine::new(method);
    let mut screen = Vec::new();
    for byte in keys.bytes() {
        let key = match byte {
            b' ' => Key::Space,
            b'<' => Key::Backspace,
            _ => Key::Char(byte),
        };
        press(&mut engine, &mut screen, key);
    }
    String::from_utf16(&screen).unwrap()
}

#[track_caller]
fn check(method: Method, cases: &[(&str, &str)]) {
    for (keys, expected) in cases {
        assert_eq!(typed(method, keys), *expected, "typing {keys:?}");
    }
}

#[test]
fn telex_words() {
    check(
        Method::Telex,
        &[
            ("tieengs", "tiếng"),
            ("Vieetj", "Việt"),
            ("nguowif", "người"),
            ("dduowngf", "đường"),
            ("ddaay", "đây"),
            ("quas", "quá"),
            ("gias", "giá"),
            ("thuyeenf", "thuyền"),
            ("khoer", "khoẻ"),
            ("thuowr", "thuở"),
            ("hoaf", "hoà"),
            ("thuyr", "thuỷ"),
            ("Xin chaof cacs banj", "Xin chào các bạn"),
        ],
    );
}

#[test]
fn telex_marks_can_come_in_any_order() {
    check(
        Method::Telex,
        &[
            ("tiesng", "tiếng".replace('ế', "é").as_str()),
            ("tiengse", "tiếng"),
            ("nguoiwf", "người"),
            ("dangd", "đang"),
        ],
    );
}

#[test]
fn telex_undo_by_repeating() {
    check(
        Method::Telex,
        &[
            ("aa", "â"),
            ("aaa", "aa"),
            ("as", "á"),
            ("ass", "as"),
            ("dd", "đ"),
            ("ddd", "dd"),
            ("w", "ư"),
            ("ww", "w"),
            ("[", "ơ"),
            ("]", "ư"),
            ("{", "Ơ"),
            ("}", "Ư"),
        ],
    );
}

#[test]
fn capitals_are_kept() {
    check(
        Method::Telex,
        &[("VIEETJ", "VIỆT"), ("Ddaay", "Đây"), ("DDAAY", "ĐÂY"), ("W", "Ư"), ("WW", "W")],
    );
    check(Method::Vni, &[("VIE6T5", "VIỆT"), ("D9ay", "Đay")]);
}

#[test]
fn foreign_words_are_restored() {
    check(
        Method::Telex,
        &[("windows", "windows"), ("system", "system"), ("process", "process"), ("ddc", "đc")],
    );
}

#[test]
fn vni_words() {
    check(
        Method::Vni,
        &[
            ("tie61ng", "tiếng"),
            ("vie65t", "việt"),
            ("nguo72i", "người"),
            ("d9uo7ng2", "đường"),
            ("a8", "ă"),
            ("a6", "â"),
            ("o7", "ơ"),
            ("u7", "ư"),
            ("hoa2", "hoà"),
            ("a11", "a1"),
            ("a66", "a6"),
            ("d99", "d9"),
            ("123", "123"),
        ],
    );
}

#[test]
fn backspace_edits_the_word() {
    check(
        Method::Telex,
        &[
            ("tieengs<", "tiến"),
            ("as<", ""),
            ("hoaf<", "ho"),
            // The tone moves when deleting a vowel changes which one carries it.
            ("tuays<", "túa"),
            // Backspacing over a space resumes the word.
            ("tieng <s", "tiéng"),
        ],
    );
}

#[test]
fn reset_ends_the_word() {
    let mut engine = Engine::new(Method::Telex);
    let mut screen = Vec::new();
    for key in [Key::Char(b'a'), Key::Reset, Key::Char(b's')] {
        press(&mut engine, &mut screen, key);
    }
    assert_eq!(String::from_utf16(&screen).unwrap(), "as");
    assert!(engine.has_state());
    engine.reset();
    assert!(!engine.has_state());
}

/// Every onset that can precede a rhyme, as (Telex keys, VNI keys, text).
const ONSETS: &[(&str, &str, &str)] = &[
    ("", "", ""),
    ("b", "b", "b"),
    ("c", "c", "c"),
    ("ch", "ch", "ch"),
    ("d", "d", "d"),
    ("dd", "d9", "đ"),
    ("g", "g", "g"),
    ("gh", "gh", "gh"),
    ("gi", "gi", "gi"),
    ("h", "h", "h"),
    ("k", "k", "k"),
    ("kh", "kh", "kh"),
    ("l", "l", "l"),
    ("m", "m", "m"),
    ("n", "n", "n"),
    ("ng", "ng", "ng"),
    ("ngh", "ngh", "ngh"),
    ("nh", "nh", "nh"),
    ("ph", "ph", "ph"),
    ("qu", "qu", "qu"),
    ("r", "r", "r"),
    ("s", "s", "s"),
    ("t", "t", "t"),
    ("th", "th", "th"),
    ("tr", "tr", "tr"),
    ("v", "v", "v"),
    ("x", "x", "x"),
];

/// Written Vietnamese rhymes without tone, grouped by nucleus.
const RHYMES: &[&str] = &[
    "a", "ac", "ach", "ai", "am", "an", "ang", "anh", "ao", "ap", "at", "au", "ay",
    "ăc", "ăm", "ăn", "ăng", "ăp", "ăt",
    "âc", "âm", "ân", "âng", "âp", "ât", "âu", "ây",
    "e", "ec", "em", "en", "eng", "eo", "ep", "et",
    "ê", "êch", "êm", "ên", "ênh", "êp", "êt", "êu",
    "i", "ia", "ich", "iêc", "iêm", "iên", "iêng", "iêp", "iêt", "iêu", "im", "in", "inh", "ip", "it", "iu",
    "o", "oa", "oac", "oach", "oai", "oam", "oan", "oang", "oanh", "oap", "oat", "oay",
    "oăc", "oăm", "oăn", "oăng", "oăt", "oc", "oe", "oen", "oeo", "oet", "oi", "om", "on", "ong", "op", "ot",
    "ô", "ôc", "ôi", "ôm", "ôn", "ông", "ôp", "ôt",
    "ơ", "ơi", "ơm", "ơn", "ơp", "ơt",
    "u", "ua", "uân", "uâng", "uât", "uây", "uc", "uê", "uêch", "uênh", "ui", "um", "un", "ung",
    "uôc", "uôi", "uôm", "uôn", "uông", "uôt", "up", "ut", "uy", "uya", "uych", "uyên", "uyêt", "uyt", "uynh", "uyu",
    "ư", "ưa", "ưc", "ưi", "ưm", "ưn", "ưng", "ươc", "ươi", "ươm", "ươn", "ương", "ươp", "ươt", "ươu", "ưt", "ưu",
    "y", "yêm", "yên", "yêng", "yêt", "yêu",
];

/// How a vowel is keyed: (plain letter, Telex mark key, VNI mark key).
fn vowel_keys(vowel: char) -> Option<(char, Option<char>, Option<char>)> {
    Some(match vowel {
        'a' | 'e' | 'i' | 'o' | 'u' | 'y' => (vowel, None, None),
        'â' => ('a', Some('a'), Some('6')),
        'ă' => ('a', Some('w'), Some('8')),
        'ê' => ('e', Some('e'), Some('6')),
        'ô' => ('o', Some('o'), Some('6')),
        'ơ' => ('o', Some('w'), Some('7')),
        'ư' => ('u', Some('w'), Some('7')),
        _ => return None,
    })
}

/// Which letter of the rhyme carries the tone, from the orthography rules with
/// the modern placement (hoà, thuý): a marked vowel wins, the second of ươ; in a
/// closed syllable or a triple the later vowel; otherwise the first, except
/// oa, oe and uy which take the second.
fn tone_index(rhyme: &[char]) -> usize {
    let vowels: Vec<usize> =
        (0..rhyme.len()).filter(|i| vowel_keys(rhyme[*i]).is_some()).collect();
    let marked: Vec<usize> =
        vowels.iter().copied().filter(|i| vowel_keys(rhyme[*i]).unwrap().1.is_some()).collect();
    if let Some(last) = marked.last() {
        return *last;
    }
    let closed = vowels.last().unwrap() + 1 < rhyme.len();
    let pair: String = vowels.iter().map(|i| rhyme[*i]).collect();
    match vowels.len() {
        1 => vowels[0],
        3 => vowels[1],
        _ if closed || matches!(pair.as_str(), "oa" | "oe" | "uy") => vowels[1],
        _ => vowels[0],
    }
}

const TONED: &[(char, [char; 5])] = &[
    ('a', ['á', 'à', 'ả', 'ã', 'ạ']),
    ('â', ['ấ', 'ầ', 'ẩ', 'ẫ', 'ậ']),
    ('ă', ['ắ', 'ằ', 'ẳ', 'ẵ', 'ặ']),
    ('e', ['é', 'è', 'ẻ', 'ẽ', 'ẹ']),
    ('ê', ['ế', 'ề', 'ể', 'ễ', 'ệ']),
    ('i', ['í', 'ì', 'ỉ', 'ĩ', 'ị']),
    ('o', ['ó', 'ò', 'ỏ', 'õ', 'ọ']),
    ('ô', ['ố', 'ồ', 'ổ', 'ỗ', 'ộ']),
    ('ơ', ['ớ', 'ờ', 'ở', 'ỡ', 'ợ']),
    ('u', ['ú', 'ù', 'ủ', 'ũ', 'ụ']),
    ('ư', ['ứ', 'ừ', 'ử', 'ữ', 'ự']),
    ('y', ['ý', 'ỳ', 'ỷ', 'ỹ', 'ỵ']),
];

/// Whether this onset and rhyme can be spelled together.
fn spellable(onset: &str, rhyme: &str) -> bool {
    let first = rhyme.chars().next().unwrap();
    let front = matches!(first, 'e' | 'ê' | 'i' | 'y');
    match onset {
        // k spells only these front rhymes; "kiu" and "kyên" are not words.
        "k" => front && rhyme != "iu" && !rhyme.starts_with("yê"),
        "gh" | "ngh" => front && first != 'y',
        "c" | "g" | "ng" => !front,
        "gi" => first != 'i',
        "qu" => !matches!(first, 'u' | 'ư' | 'o' | 'ô' | 'ơ') || rhyme == "y",
        _ => true,
    }
}

/// Telex and VNI spell the same plain letters; only the mark keys differ.
fn keys_for(onset: &str, rhyme: &[char], telex: bool, tone: Option<char>) -> String {
    let mut keys = String::from(onset);
    for c in rhyme {
        match vowel_keys(*c) {
            Some((plain, telex_mark, vni_mark)) => {
                keys.push(plain);
                keys.extend(if telex { telex_mark } else { vni_mark });
            }
            None => keys.push(*c),
        }
    }
    keys.extend(tone);
    keys
}

#[test]
fn every_syllable_types_correctly() {
    let telex_tones = ['s', 'f', 'r', 'x', 'j'];
    let vni_tones = ['1', '2', '3', '4', '5'];
    let mut tested = 0;
    let mut failures = Vec::new();

    for (telex_onset, vni_onset, text_onset) in ONSETS {
        for rhyme_text in RHYMES {
            if !spellable(text_onset, rhyme_text) {
                continue;
            }
            let rhyme: Vec<char> = rhyme_text.chars().collect();
            let stop_final = rhyme_text.ends_with(['c', 'p', 't']) || rhyme_text.ends_with("ch");

            for tone in 0..=5usize {
                // Stop finals only take sắc and nặng.
                if stop_final && !matches!(tone, 0 | 1 | 5) {
                    continue;
                }
                let mut expected: Vec<char> = rhyme.clone();
                if tone > 0 {
                    let index = tone_index(&rhyme);
                    let (_, forms) = TONED.iter().find(|(c, _)| *c == rhyme[index]).unwrap();
                    expected[index] = forms[tone - 1];
                }
                let expected = format!("{text_onset}{}", expected.iter().collect::<String>());

                let tone_key = |keys: &[char]| (tone > 0).then(|| keys[tone - 1]);
                let telex = keys_for(telex_onset, &rhyme, true, tone_key(&telex_tones));
                let vni = keys_for(vni_onset, &rhyme, false, tone_key(&vni_tones));
                // ươ has a shorthand: one w after the o marks both letters.
                // Except after th, where uow must stay uơ to spell "thuở".
                let keeps_uo = *text_onset == "th" && matches!(*rhyme_text, "ươi" | "ươu");
                let telex_short = if keeps_uo { telex.clone() } else { telex.replace("uwow", "uow") };
                for (method, keys) in
                    [(Method::Telex, telex), (Method::Telex, telex_short), (Method::Vni, vni)]
                {
                    tested += 1;
                    let actual = typed(method, &keys);
                    if actual != expected {
                        failures.push(format!("{method:?} {keys}: got {actual}, want {expected}"));
                    }
                }
            }
        }
    }

    assert!(tested > 15_000, "only {tested} syllables tested");
    assert!(
        failures.is_empty(),
        "{} of {tested} failed, first few:\n{}",
        failures.len(),
        failures[..failures.len().min(40)].join("\n")
    );
}

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

#[test]
fn survives_arbitrary_input() {
    for method in [Method::Telex, Method::Vni] {
        let mut engine = Engine::new(method);
        let mut screen = Vec::new();
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        for _ in 0..2_000_000 {
            let key = match rng.below(40) {
                0 => Key::Space,
                1 | 2 => Key::Backspace,
                3 => Key::Reset,
                _ => Key::Char(0x21 + rng.below(0x5E) as u8),
            };
            press(&mut engine, &mut screen, key);
            if screen.len() > 4096 {
                screen.clear();
                engine.reset();
            }
        }
    }
}

/// Repeated tone keys grow the keystroke log without growing the word. Upstream
/// overruns its buffer here when the word is then restored.
#[test]
fn endless_marks_on_one_letter_do_not_overflow() {
    for tail in ["k", "kk", "w"] {
        let keys = format!("a{}{tail}", "sf".repeat(500));
        typed(Method::Telex, &keys);
    }
}

#[test]
fn very_long_runs_keep_working() {
    let long = "x".repeat(1000);
    assert_eq!(typed(Method::Telex, &format!("{long} as")), format!("{long} á"));
}
