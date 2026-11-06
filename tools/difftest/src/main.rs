//! Differential test between minikey and the upstream VnKey engine.
//!
//! Both engines are driven exactly the way their keyboard hooks drive them, and
//! the text they would leave on screen is compared after every keystroke.
//!
//! Usage: `cargo run --release [-- <random-sequences-per-thread> <exhaustive-depth>]`

use minikey::{Engine, Method};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use vnkey_engine::engine::Engine as Upstream;
use vnkey_engine::input::InputMethod;

#[derive(Clone, Copy, Debug)]
enum Key {
    Char(u8),
    Space,
    Backspace,
    /// Anything that moves the caret: a click, an arrow key, Enter.
    Reset,
}

/// Fewer keys than the smallest engine buffer, so no run hits either engine's
/// different overflow handling. Upstream's panics on overflow; see `tests/`.
const MAX_RUN: usize = 56;

trait Driver {
    fn new(method: Method) -> Self;
    fn press(&mut self, key: Key, screen: &mut Vec<u16>);
}

fn apply(screen: &mut Vec<u16>, backspaces: usize, text: impl IntoIterator<Item = u16>) {
    screen.truncate(screen.len().saturating_sub(backspaces));
    screen.extend(text);
}

impl Driver for Upstream {
    fn new(method: Method) -> Self {
        let mut engine = Upstream::new();
        engine.set_input_method(match method {
            Method::Telex => InputMethod::Telex,
            Method::Vni => InputMethod::Vni,
        });
        engine
    }

    /// Mirrors upstream's `ll_keyboard_proc` and `handle_backspace`.
    fn press(&mut self, key: Key, screen: &mut Vec<u16>) {
        match key {
            Key::Char(ascii) => {
                let result = self.process(u32::from(ascii));
                if result.processed && (result.backspaces > 0 || !result.output.is_empty()) {
                    let text = String::from_utf8(result.output).unwrap();
                    apply(screen, result.backspaces, text.encode_utf16());
                } else {
                    screen.push(u16::from(ascii));
                }
            }
            Key::Space => {
                self.soft_reset();
                screen.push(u16::from(b' '));
            }
            Key::Backspace => {
                let result = self.process_backspace();
                if result.processed && result.backspaces > 1 {
                    let text = String::from_utf8(result.output).unwrap();
                    apply(screen, result.backspaces, text.encode_utf16());
                } else {
                    screen.pop();
                }
            }
            Key::Reset => self.reset(),
        }
    }
}

impl Driver for Engine {
    fn new(method: Method) -> Self {
        Engine::new(method)
    }

    fn press(&mut self, key: Key, screen: &mut Vec<u16>) {
        match key {
            Key::Char(ascii) => match self.key(ascii) {
                Some(edit) => apply(screen, edit.backspaces, edit.text().iter().copied()),
                None => screen.push(u16::from(ascii)),
            },
            Key::Space => {
                self.end_word();
                screen.push(u16::from(b' '));
            }
            Key::Backspace => match self.backspace() {
                Some(edit) => apply(screen, edit.backspaces, edit.text().iter().copied()),
                None => {
                    screen.pop();
                }
            },
            Key::Reset => self.reset(),
        }
    }
}

/// Two known upstream bugs, both fixed here, and both in restoring a word from
/// its keystrokes. Once one has struck the screens differ, so the sequence ends.
fn upstream_restored_wrongly(method: Method, keys: &[Key], theirs: &[u16], ours: &[u16]) -> bool {
    // A key that went into a deleted letter stays in upstream's log: "ax<we"
    // gives "awe". `tests/typing.rs` checks this behaviour without upstream.
    let backspaced = word_contains(keys, |key| matches!(key, Key::Backspace));
    // Restoring stops at a `[` or `]` typed as a letter, losing everything
    // before it: "x[0" leaves "0".
    let bracket = word_contains(keys, |key| matches!(key, Key::Char(b'[' | b']' | b'{' | b'}')));
    backspaced || (method == Method::Telex && bracket && theirs.len() < ours.len())
}

/// Whether any key of the word at the end of `keys` matches. A space ends a
/// word, unless a backspace right after it resumed that word.
fn word_contains(keys: &[Key], matches: impl Fn(&Key) -> bool) -> bool {
    let mut resumed = false;
    for key in keys.iter().rev() {
        if matches(key) {
            return true;
        }
        match key {
            Key::Reset => return false,
            Key::Space if !resumed => return false,
            _ => {}
        }
        resumed = matches!(key, Key::Backspace);
    }
    false
}

/// Runs one sequence through both engines; returns the first divergence.
fn compare(method: Method, keys: &[Key]) -> Option<String> {
    let mut theirs = <Upstream as Driver>::new(method);
    let mut ours = <Engine as Driver>::new(method);
    let (mut their_screen, mut our_screen) = (Vec::new(), Vec::new());
    for (i, key) in keys.iter().enumerate() {
        theirs.press(*key, &mut their_screen);
        ours.press(*key, &mut our_screen);
        if their_screen != our_screen {
            if upstream_restored_wrongly(method, &keys[..=i], &their_screen, &our_screen) {
                return None;
            }
            return Some(format!(
                "{method:?} diverged at key {i} of {:?}\n  upstream: {:?}\n  minikey:  {:?}",
                &keys[..=i],
                String::from_utf16_lossy(&their_screen),
                String::from_utf16_lossy(&our_screen),
            ));
        }
    }
    None
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const ONSETS: &[&str] = &[
    "", "b", "c", "ch", "d", "dd", "g", "gh", "gi", "h", "k", "kh", "l", "m", "n", "ng", "ngh",
    "nh", "p", "ph", "q", "qu", "r", "s", "t", "th", "tr", "v", "x", "dz",
];
const NUCLEI: &[&str] = &[
    "a", "e", "i", "o", "u", "y", "ai", "ao", "au", "ay", "eo", "eu", "ia", "ie", "iu", "oa",
    "oe", "oi", "ua", "ue", "ui", "uo", "uu", "uy", "ye", "ieu", "oai", "oay", "oeo", "uay",
    "uoi", "uou", "uya", "uye", "uyu", "yeu",
];
const CODAS: &[&str] = &["", "", "", "c", "ch", "m", "n", "ng", "nh", "p", "t"];

const TELEX_MARKS: &[u8] = b"sfrxjzwaeod";
const VNI_MARKS: &[u8] = b"0123456789";
const PLAIN: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789,.;'[]{}-=!?/\\`~@#$%^&*()_+<>:\"|";

/// A syllable-shaped run with marks sprinkled through it.
fn push_syllable(rng: &mut Rng, method: Method, keys: &mut Vec<Key>) {
    let marks = if method == Method::Telex { TELEX_MARKS } else { VNI_MARKS };
    let uppercase = rng.below(8) == 0;
    let mut letters = Vec::new();
    letters.extend_from_slice(rng.pick(ONSETS).as_bytes());
    letters.extend_from_slice(rng.pick(NUCLEI).as_bytes());
    letters.extend_from_slice(rng.pick(CODAS).as_bytes());
    for letter in letters {
        keys.push(Key::Char(if uppercase { letter.to_ascii_uppercase() } else { letter }));
        if rng.below(3) == 0 {
            keys.push(Key::Char(*rng.pick(marks)));
        }
    }
    for _ in 0..rng.below(4) {
        keys.push(Key::Char(*rng.pick(marks)));
    }
}

fn random_sequence(rng: &mut Rng, method: Method) -> Vec<Key> {
    let mut keys = Vec::new();
    while keys.len() < MAX_RUN - 16 {
        match rng.below(10) {
            0..=4 => push_syllable(rng, method, &mut keys),
            5 => keys.push(Key::Space),
            6 => keys.push(Key::Reset),
            7 => {
                for _ in 0..=rng.below(4) {
                    keys.push(Key::Backspace);
                }
            }
            _ => {
                for _ in 0..=rng.below(6) {
                    keys.push(Key::Char(*rng.pick(PLAIN)));
                }
            }
        }
    }
    keys
}

fn report(failures: &AtomicU64, message: String) {
    if failures.fetch_add(1, Ordering::Relaxed) < 20 {
        println!("{message}\n");
    }
}

/// Every sequence of `depth` keys drawn from `alphabet`.
fn exhaustive(method: Method, alphabet: &[Key], depth: u32, prefix: Key, failures: &AtomicU64) -> u64 {
    let total = (alphabet.len() as u64).pow(depth - 1);
    let mut keys = vec![prefix; depth as usize];
    for mut n in 0..total {
        for slot in keys.iter_mut().skip(1) {
            *slot = alphabet[(n % alphabet.len() as u64) as usize];
            n /= alphabet.len() as u64;
        }
        if let Some(message) = compare(method, &keys) {
            report(failures, message);
        }
    }
    total
}

fn alphabet(method: Method) -> Vec<Key> {
    let chars: &[u8] = match method {
        Method::Telex => b"aeiouydgnhqtwsfrxjz[]",
        Method::Vni => b"aeiouydgnhqt0123456789",
    };
    let mut keys: Vec<Key> = chars.iter().map(|c| Key::Char(*c)).collect();
    keys.extend([Key::Space, Key::Backspace]);
    keys
}

fn main() {
    let mut args = std::env::args().skip(1);
    let sequences: u64 = args.next().map_or(2_000_000, |a| a.parse().unwrap());
    let depth: u32 = args.next().map_or(6, |a| a.parse().unwrap());
    let failures = AtomicU64::new(0);

    let tested = thread::scope(|scope| {
        let mut workers = Vec::new();
        for method in [Method::Telex, Method::Vni] {
            let keys = alphabet(method);
            for prefix in keys.clone() {
                let (keys, failures) = (keys.clone(), &failures);
                workers.push(scope.spawn(move || exhaustive(method, &keys, depth, prefix, failures)));
            }
            for seed in 1..=8u64 {
                let failures = &failures;
                workers.push(scope.spawn(move || {
                    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
                    for _ in 0..sequences {
                        let keys = random_sequence(&mut rng, method);
                        if let Some(message) = compare(method, &keys) {
                            report(failures, message);
                        }
                    }
                    sequences
                }));
            }
        }
        workers.into_iter().map(|w| w.join().unwrap()).sum::<u64>()
    });

    let failed = failures.load(Ordering::Relaxed);
    println!("{tested} sequences compared, {failed} diverged");
    std::process::exit(i32::from(failed > 0));
}
