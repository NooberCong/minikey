//! The typing engine: a state machine that turns ASCII keystrokes into edits
//! of the word being typed.
//!
//! Ported from the VnKey engine (itself derived from UniKey) with its options
//! fixed at their defaults: free tone placement, modern tone style ("hoà"),
//! spell checking, and automatic restore of non-Vietnamese words. It never
//! allocates, so a keystroke costs no more than a few table lookups.

mod lexicon;

use lexicon::{is_valid_cv, is_valid_cvc, Cs, Sym, Vs};

/// Most letters tracked before the engine gives up on the current run of text.
/// Far longer than any Vietnamese word; a run this long is a URL or similar.
pub const CAPACITY: usize = 64;

/// The typing convention.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Telex,
    Vni,
}

/// A change to the text on screen: delete `backspaces` characters before the
/// caret, then insert [`text`](Edit::text).
#[derive(Clone, Copy, Debug)]
pub struct Edit {
    pub backspaces: usize,
    units: [u16; CAPACITY],
    len: usize,
}

impl Edit {
    /// The replacement text as UTF-16.
    pub fn text(&self) -> &[u16] {
        &self.units[..self.len]
    }
}

/// What a key asks the engine to do.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Tone(u8),
    /// Add or remove ^ on any vowel (VNI 6).
    RoofAny,
    /// Add or remove ^ on one specific vowel (Telex aa, ee, oo).
    Roof(Sym),
    /// Add or remove a horn or breve on any vowel.
    HookAny,
    /// Add or remove a horn on u or o (VNI 7).
    HookUo,
    /// Add or remove the breve on a (VNI 8).
    Bowl,
    /// Telex `w`: a hook, or a standalone ư.
    TelexW,
    /// Toggle đ.
    Dd,
    /// Telex `[` and `]`: type ơ or ư directly.
    Map(Sym, bool),
    Plain,
}

/// How a character behaves outside of its [`Action`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum CharClass {
    /// May be part of a Vietnamese word.
    Vietnamese,
    /// Ends the word.
    WordBreak,
    /// Letters and digits that no Vietnamese word contains.
    Foreign,
}

fn char_class(key: u8) -> CharClass {
    match key {
        b',' | b';' | b':' | b'.' | b'"' | b'\'' | b'!' | b'?' | b'<' | b'>' | b'=' | b'+'
        | b'-' | b'*' | b'/' | b'\\' | b'_' | b'@' | b'#' | b'$' | b'%' | b'&' | b'(' | b')'
        | b'{' | b'}' | b'[' | b']' | b'|' => CharClass::WordBreak,
        b'f' | b'j' | b'w' | b'F' | b'J' | b'W' => CharClass::Foreign,
        _ if key.is_ascii_alphabetic() => CharClass::Vietnamese,
        _ => CharClass::Foreign,
    }
}

fn is_bracket(key: u8) -> bool {
    matches!(key, b'[' | b']' | b'{' | b'}')
}

impl Method {
    fn action(self, key: u8) -> Action {
        match self {
            Method::Telex => match key.to_ascii_lowercase() {
                b'z' => Action::Tone(0),
                b's' => Action::Tone(1),
                b'f' => Action::Tone(2),
                b'r' => Action::Tone(3),
                b'x' => Action::Tone(4),
                b'j' => Action::Tone(5),
                b'w' => Action::TelexW,
                b'a' => Action::Roof(Sym::Aa),
                b'e' => Action::Roof(Sym::Ee),
                b'o' => Action::Roof(Sym::Oo),
                b'd' => Action::Dd,
                b'[' => Action::Map(Sym::Ow, false),
                b']' => Action::Map(Sym::Uw, false),
                b'{' => Action::Map(Sym::Ow, true),
                b'}' => Action::Map(Sym::Uw, true),
                _ => Action::Plain,
            },
            Method::Vni => match key {
                b'0'..=b'5' => Action::Tone(key - b'0'),
                b'6' => Action::RoofAny,
                b'7' => Action::HookUo,
                b'8' => Action::Bowl,
                b'9' => Action::Dd,
                _ => Action::Plain,
            },
        }
    }
}

/// One keystroke, classified.
#[derive(Clone, Copy)]
struct Event {
    action: Action,
    class: CharClass,
    sym: Sym,
    caps: bool,
    key: u8,
}

/// What the word looks like up to and including a buffer entry.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    /// Not a Vietnamese word.
    Foreign,
    /// A word break.
    Empty,
    /// Consonants only.
    C,
    /// Vowels only.
    V,
    /// Consonants then vowels.
    Cv,
    /// Vowels then consonants.
    Vc,
    /// Consonants, vowels, consonants.
    Cvc,
}

/// One on-screen character of the word, with the parse of the word so far.
/// Offsets count back from this entry to the end of the named part, or are -1.
#[derive(Clone, Copy)]
struct Entry {
    form: Form,
    c1_offset: i32,
    c2_offset: i32,
    v_offset: i32,
    vs: Vs,
    cs: Cs,
    sym: Sym,
    caps: bool,
    tone: u8,
    key: u8,
}

const BLANK: Entry = Entry {
    form: Form::Empty,
    c1_offset: -1,
    c2_offset: -1,
    v_offset: -1,
    vs: Vs::Nil,
    cs: Cs::Nil,
    sym: Sym::NonVn,
    caps: false,
    tone: 0,
    key: 0,
};

/// Word state to return to if the user backspaces over the space that ended it.
#[derive(Clone, Copy)]
struct Saved {
    current: i32,
    keys_current: i32,
    single_mode: bool,
}

pub struct Engine {
    method: Method,
    buffer: [Entry; CAPACITY],
    /// Index of the last entry, -1 when empty.
    current: i32,
    /// First entry changed by this keystroke; the edit starts here.
    change_pos: i32,
    /// On-screen characters to delete, from the caret back to `change_pos`.
    backspaces: usize,
    /// The word stopped being Vietnamese but `dd` still applies, as in "đc".
    single_mode: bool,
    /// Raw keystrokes of the current run, for restoring foreign words.
    keys: [u8; CAPACITY],
    keys_current: i32,
    saved: Option<Saved>,
}

impl Engine {
    pub const fn new(method: Method) -> Self {
        Self {
            method,
            buffer: [BLANK; CAPACITY],
            current: -1,
            change_pos: 0,
            backspaces: 0,
            single_mode: false,
            keys: [0; CAPACITY],
            keys_current: -1,
            saved: None,
        }
    }

    pub fn method(&self) -> Method {
        self.method
    }

    pub fn set_method(&mut self, method: Method) {
        self.method = method;
        self.reset();
    }

    /// Forgets the current word, for when the caret may have moved.
    pub fn reset(&mut self) {
        self.current = -1;
        self.keys_current = -1;
        self.single_mode = false;
        self.saved = None;
    }

    /// Ends the current word at a space. A following backspace resumes it.
    pub fn end_word(&mut self) {
        self.saved = (self.current >= 0).then_some(Saved {
            current: self.current,
            keys_current: self.keys_current,
            single_mode: self.single_mode,
        });
        self.current = -1;
        self.keys_current = -1;
        self.single_mode = false;
    }

    /// Whether any typing state exists that a caret move would invalidate.
    pub fn has_state(&self) -> bool {
        self.current >= 0 || self.saved.is_some()
    }

    /// Feeds a printable ASCII key other than space. Returns the edit that
    /// replaces it, or `None` if the key should simply be typed as is.
    pub fn key(&mut self, key: u8) -> Option<Edit> {
        self.saved = None;
        if self.current + 2 >= CAPACITY as i32 || self.keys_current + 2 >= CAPACITY as i32 {
            self.reset();
        }
        self.backspaces = 0;
        self.change_pos = self.current + 1;

        let event = self.classify(key);
        let changed = self.dispatch(event);

        if self.current >= 0
            && self.single_mode
            && self.at(self.current).form == Form::Foreign
            && event.class == CharClass::Vietnamese
        {
            self.reparse_single();
        }
        if self.current >= 0 {
            self.keys_current += 1;
            self.keys[self.keys_current as usize] = key;
        }

        if changed || self.restore_foreign_word() {
            self.edit()
        } else {
            None
        }
    }

    /// Feeds the backspace key. Returns the edit that replaces it, or `None`
    /// if a plain backspace is what the text needs.
    pub fn backspace(&mut self) -> Option<Edit> {
        if self.current < 0 {
            if let Some(saved) = self.saved.take() {
                self.current = saved.current;
                self.keys_current = saved.keys_current;
                self.single_mode = saved.single_mode;
            }
            return None;
        }

        self.backspaces = 0;
        self.change_pos = self.current + 1;
        self.mark_change(self.current);

        let entry = *self.at(self.current);
        let prev_form = if self.current > 0 { self.at(self.current - 1).form } else { Form::Empty };

        if self.current == 0
            || matches!(entry.form, Form::Empty | Form::Foreign | Form::C)
            || matches!(prev_form, Form::C | Form::Cvc | Form::Vc)
        {
            self.drop_last();
            return None;
        }

        // Deleting a vowel can move the tone onto a different letter.
        let v_end = self.current - entry.v_offset;
        let vs = self.at(v_end).vs;
        let v_start = v_end - vs.info().len + 1;
        let new_vs = self.at(self.current - 1).vs;
        let cur_tone_pos = v_start + tone_position(vs, v_end == self.current);
        let new_tone_pos = v_start + tone_position(new_vs, true);
        let tone = self.at(cur_tone_pos).tone;

        if tone == 0 || cur_tone_pos == new_tone_pos || cur_tone_pos == self.current {
            self.drop_last();
            return None;
        }

        self.mark_change(new_tone_pos);
        self.at_mut(new_tone_pos).tone = tone;
        self.mark_change(cur_tone_pos);
        self.at_mut(cur_tone_pos).tone = 0;
        self.drop_last();
        self.edit()
    }

    fn classify(&self, key: u8) -> Event {
        let action = self.method.action(key);
        match action {
            Action::Map(sym, caps) => {
                Event { action, class: CharClass::Vietnamese, sym, caps, key }
            }
            _ => Event {
                action,
                class: char_class(key),
                sym: Sym::from_ascii(key),
                caps: key.is_ascii_uppercase(),
                key,
            },
        }
    }

    fn dispatch(&mut self, event: Event) -> bool {
        match event.action {
            Action::Tone(tone) => self.apply_tone(event, tone),
            Action::RoofAny | Action::Roof(_) => self.apply_roof(event),
            Action::HookAny | Action::HookUo | Action::Bowl => self.apply_hook(event),
            Action::TelexW => self.apply_telex_w(event),
            Action::Dd => self.apply_dd(event),
            Action::Map(..) => self.apply_map(event),
            Action::Plain => self.append(event),
        }
    }

    fn at(&self, index: i32) -> &Entry {
        &self.buffer[index as usize]
    }

    fn at_mut(&mut self, index: i32) -> &mut Entry {
        &mut self.buffer[index as usize]
    }

    /// Extends the edit back to `pos`.
    fn mark_change(&mut self, pos: i32) {
        if pos < self.change_pos {
            self.backspaces += (self.change_pos - pos) as usize;
            self.change_pos = pos;
        }
    }

    /// Renders everything from `change_pos` on, or `None` if nothing changed.
    fn edit(&self) -> Option<Edit> {
        let mut units = [0; CAPACITY];
        let mut len = 0;
        for index in self.change_pos..=self.current {
            let entry = self.at(index);
            units[len] = entry.sym.to_utf16(entry.tone, entry.caps).unwrap_or(u16::from(entry.key));
            len += 1;
        }
        (self.backspaces > 0 || len > 0).then_some(Edit { backspaces: self.backspaces, units, len })
    }

    /// Removes the last entry and its keystroke.
    fn drop_last(&mut self) {
        self.current -= 1;
        if self.keys_current >= 0 {
            self.keys_current -= 1;
        }
        if self.current >= 0 && self.at(self.current).form == Form::Empty {
            while self.keys_current >= 0
                && char_class(self.keys[self.keys_current as usize]) != CharClass::WordBreak
            {
                self.keys_current -= 1;
            }
        }
    }

    /// After `dd` revived a foreign word, parses the letter just typed as the
    /// start of a new syllable.
    fn reparse_single(&mut self) {
        let sym = self.at(self.current).sym;
        let entry = self.at_mut(self.current);
        entry.c2_offset = -1;
        if sym.is_vowel() {
            entry.form = Form::V;
            entry.v_offset = 0;
            entry.vs = Vs::lookup1(sym);
            entry.c1_offset = -1;
        } else {
            entry.form = Form::C;
            entry.c1_offset = 0;
            entry.v_offset = -1;
            entry.cs = Cs::lookup([sym, Sym::NonVn, Sym::NonVn]);
        }
    }

    /// When a word turns out not to be Vietnamese, replaces whatever marks it
    /// picked up with the keys as typed: "ưindơ" goes back to "window".
    fn restore_foreign_word(&mut self) -> bool {
        if self.current < 0 || self.at(self.current).form != Form::Foreign {
            return false;
        }

        let mut word_start = self.current;
        while word_start > 0 && self.at(word_start - 1).form != Form::Empty {
            word_start -= 1;
        }

        let word = &self.buffer[word_start as usize..self.current as usize];
        let has_vowel = word.iter().any(|e| e.sym.is_vowel());
        let has_marks = word.iter().any(|e| {
            e.tone != 0
                || matches!(e.sym, Sym::Aa | Sym::Ee | Sym::Oo | Sym::Ow | Sym::Uw | Sym::Aw | Sym::Dd)
        });
        // Consonant-only words stay: "đc" and "đk" are common abbreviations.
        if !has_marks || !has_vowel {
            return false;
        }

        // Telex `[` and `]` look like word breaks in the keystroke log, but those
        // that became ơ and ư are letters of this word: "x[0" must keep its "x[".
        let mut bracket_letters = word.iter().filter(|e| is_bracket(e.key)).count();
        let mut keys_start = self.keys_current;
        while keys_start > 0 {
            if char_class(self.keys[keys_start as usize - 1]) == CharClass::WordBreak {
                if bracket_letters == 0 {
                    break;
                }
                bracket_letters -= 1;
            }
            keys_start -= 1;
        }
        let key_count = self.keys_current - keys_start + 1;
        if word_start + key_count > CAPACITY as i32 {
            return false;
        }

        self.backspaces = (self.current - word_start) as usize;
        self.change_pos = word_start;
        self.current = word_start - 1;
        for i in 0..key_count {
            self.current += 1;
            let key = self.keys[(keys_start + i) as usize];
            *self.at_mut(self.current) = Entry { form: Form::Foreign, key, ..BLANK };
        }
        true
    }

    // ---- Appending ------------------------------------------------------

    /// Adds a key with no special meaning. Returns whether the text needs an
    /// edit, i.e. whether the key cannot just be typed as is.
    fn append(&mut self, event: Event) -> bool {
        match event.class {
            CharClass::WordBreak => {
                self.single_mode = false;
                self.current += 1;
                *self.at_mut(self.current) = Entry { key: event.key, ..BLANK };
                false
            }
            CharClass::Foreign => {
                self.current += 1;
                let entry = self.at_mut(self.current);
                entry.key = event.key;
                entry.sym = event.sym;
                entry.tone = 0;
                entry.caps = event.caps;
                self.set_foreign(self.current);
                false
            }
            CharClass::Vietnamese if event.sym.is_vowel() => {
                // The u of "qu" and the i of "gi" belong to the consonant.
                if self.current >= 0 && self.at(self.current).form == Form::C {
                    let cs = self.at(self.current).cs;
                    if (cs == Cs::Q && event.sym == Sym::U) || (cs == Cs::G && event.sym == Sym::I) {
                        return self.append_consonant(event);
                    }
                }
                self.append_vowel(event)
            }
            CharClass::Vietnamese => self.append_consonant(event),
        }
    }

    fn set_foreign(&mut self, index: i32) {
        let entry = self.at_mut(index);
        entry.form = Form::Foreign;
        entry.c1_offset = -1;
        entry.c2_offset = -1;
        entry.v_offset = -1;
    }

    /// Records entry `index` as extending a word of shape `form`.
    fn set_shape(&mut self, index: i32, form: Form, c1_offset: i32, c2_offset: i32, v_offset: i32) {
        let entry = self.at_mut(index);
        entry.form = form;
        entry.c1_offset = c1_offset;
        entry.c2_offset = c2_offset;
        entry.v_offset = v_offset;
    }

    fn append_vowel(&mut self, event: Event) -> bool {
        self.current += 1;
        let cur = self.current;
        let entry = self.at_mut(cur);
        entry.sym = event.sym;
        entry.caps = event.caps;
        entry.tone = 0;
        entry.key = event.key;

        let prev_form = if cur == 0 { Form::Empty } else { self.at(cur - 1).form };
        match prev_form {
            Form::Empty => {
                self.set_shape(cur, Form::V, -1, -1, 0);
                self.at_mut(cur).vs = Vs::lookup1(event.sym);
            }
            Form::Foreign | Form::Cvc | Form::Vc => self.set_foreign(cur),
            Form::V | Form::Cv => {
                let prev = *self.at(cur - 1);
                let info = prev.vs.info();
                let prev_tone_pos = cur - info.len + tone_position(prev.vs, true);
                let tone = self.at(prev_tone_pos).tone;

                let new_vs = match info.len {
                    3 => Vs::Nil,
                    2 => Vs::lookup([info.letters[0], info.letters[1], event.sym]),
                    _ => Vs::lookup([info.letters[0], event.sym, Sym::NonVn]),
                };
                let cv_ok = prev_form != Form::Cv
                    || is_valid_cv(self.at(cur - 1 - prev.c1_offset).cs, new_vs);

                if new_vs == Vs::Nil || !cv_ok {
                    self.set_foreign(cur);
                } else {
                    let c1_offset = if prev_form == Form::Cv { prev.c1_offset + 1 } else { -1 };
                    self.set_shape(cur, prev_form, c1_offset, -1, 0);
                    self.at_mut(cur).vs = new_vs;

                    if tone != 0 {
                        let new_tone_pos = cur - info.len + tone_position(new_vs, true);
                        if new_tone_pos != prev_tone_pos {
                            self.mark_change(prev_tone_pos);
                            self.at_mut(prev_tone_pos).tone = 0;
                            self.mark_change(new_tone_pos);
                            self.at_mut(new_tone_pos).tone = tone;
                            return true;
                        }
                    }
                }
            }
            Form::C => {
                let new_vs = Vs::lookup1(event.sym);
                let prev = *self.at(cur - 1);
                if !is_valid_cv(prev.cs, new_vs) {
                    self.set_foreign(cur);
                } else {
                    self.set_shape(cur, Form::Cv, 1, -1, 0);
                    self.at_mut(cur).vs = new_vs;

                    // "gì" typed as gi + tone, then a vowel: the tone moves on.
                    if prev.cs == Cs::Gi && prev.tone != 0 {
                        self.at_mut(cur).tone = prev.tone;
                        self.mark_change(cur - 1);
                        self.at_mut(cur - 1).tone = 0;
                        return true;
                    }
                }
            }
        }

        // Only `[` and `]` reach here as non-letters; those always need an edit.
        if event.key.is_ascii_alphabetic() {
            return false;
        }
        self.mark_change(cur);
        true
    }

    fn append_consonant(&mut self, event: Event) -> bool {
        self.current += 1;
        let cur = self.current;
        let entry = self.at_mut(cur);
        entry.sym = event.sym;
        entry.caps = event.caps;
        entry.key = event.key;
        entry.tone = 0;

        let prev_form = if cur == 0 { Form::Empty } else { self.at(cur - 1).form };
        match prev_form {
            Form::Foreign => self.set_foreign(cur),
            Form::Empty => {
                self.set_shape(cur, Form::C, 0, -1, -1);
                self.at_mut(cur).cs = Cs::lookup([event.sym, Sym::NonVn, Sym::NonVn]);
            }
            Form::V | Form::Cv => return self.append_coda(event, prev_form),
            Form::C | Form::Vc | Form::Cvc => {
                let prev = *self.at(cur - 1);
                let (letters, len) = prev.cs.letters();
                let new_cs = match len {
                    3 => Cs::Nil,
                    2 => Cs::lookup([letters[0], letters[1], event.sym]),
                    _ => Cs::lookup([letters[0], event.sym, Sym::NonVn]),
                };
                let valid = new_cs != Cs::Nil
                    && (prev_form == Form::C
                        || is_valid_cvc(
                            self.initial_of(cur - 1),
                            self.at(cur - 1 - prev.v_offset).vs,
                            new_cs,
                        ));
                if !valid {
                    self.set_foreign(cur);
                } else {
                    match prev_form {
                        Form::C => self.set_shape(cur, Form::C, 0, -1, -1),
                        Form::Vc => self.set_shape(cur, Form::Vc, -1, 0, prev.v_offset + 1),
                        _ => self.set_shape(cur, Form::Cvc, prev.c1_offset + 1, 0, prev.v_offset + 1),
                    }
                    self.at_mut(cur).cs = new_cs;
                }
            }
        }
        false
    }

    /// Appends the first final consonant to a word that ended in a vowel.
    fn append_coda(&mut self, event: Event, prev_form: Form) -> bool {
        let cur = self.current;
        let prev = *self.at(cur - 1);
        // A consonant after ưo or uơ completes it to ươ.
        let new_vs = if matches!(prev.vs, Vs::UOw | Vs::UwO) { Vs::UwOw } else { prev.vs };
        let new_cs = Cs::lookup([event.sym, Sym::NonVn, Sym::NonVn]);

        if !is_valid_cvc(self.initial_of(cur - 1), new_vs, new_cs) {
            self.set_foreign(cur);
            return false;
        }

        let mut completed = false;
        if prev.vs == Vs::UwO {
            self.mark_change(cur - 1);
            let entry = self.at_mut(cur - 1);
            entry.sym = Sym::Ow;
            entry.vs = Vs::UwOw;
            completed = true;
        } else if prev.vs == Vs::UOw {
            self.mark_change(cur - 2);
            let first = self.at_mut(cur - 2);
            first.sym = Sym::Uw;
            first.vs = Vs::Uw;
            self.at_mut(cur - 1).vs = Vs::UwOw;
            completed = true;
        }

        if prev_form == Form::V {
            self.set_shape(cur, Form::Vc, -1, 0, 1);
        } else {
            self.set_shape(cur, Form::Cvc, prev.c1_offset + 1, 0, 1);
        }
        self.at_mut(cur).cs = new_cs;

        // A closed syllable can carry its tone on a different vowel.
        let old_tone_pos = cur - prev.vs.info().len + tone_position(prev.vs, true);
        let tone = self.at(old_tone_pos).tone;
        if tone != 0 {
            let new_tone_pos = cur - new_vs.info().len + tone_position(new_vs, false);
            if new_tone_pos != old_tone_pos {
                self.mark_change(new_tone_pos);
                self.at_mut(new_tone_pos).tone = tone;
                self.mark_change(old_tone_pos);
                self.at_mut(old_tone_pos).tone = 0;
                return true;
            }
        }
        completed
    }

    /// The initial consonant of the word ending at `index`.
    fn initial_of(&self, index: i32) -> Cs {
        let offset = self.at(index).c1_offset;
        if offset == -1 { Cs::Nil } else { self.at(index - offset).cs }
    }

    /// The final consonant of the word ending at `index`.
    fn final_of(&self, index: i32) -> Cs {
        let offset = self.at(index).c2_offset;
        if offset == -1 { Cs::Nil } else { self.at(index - offset).cs }
    }

    // ---- Marks ----------------------------------------------------------

    /// Undoes a mark that was just re-typed, then appends the key literally:
    /// "aa" makes â, and a third a makes "aa".
    fn append_after_undo(&mut self, event: Event) -> bool {
        self.single_mode = false;
        self.append(event);
        true
    }

    fn apply_tone(&mut self, event: Event, tone: u8) -> bool {
        if self.current < 0 {
            return self.append(event);
        }

        let last = *self.at(self.current);
        if last.form == Form::C && matches!(last.cs, Cs::Gi | Cs::Gin) {
            let pos = if last.cs == Cs::Gi { self.current } else { self.current - 1 };
            return self.set_tone(event, pos, tone);
        }
        if last.v_offset < 0 {
            return self.append(event);
        }

        // Stop-final syllables only take sắc and nặng.
        if matches!(last.form, Form::Vc | Form::Cvc)
            && matches!(last.cs, Cs::C | Cs::Ch | Cs::P | Cs::T)
            && matches!(tone, 2..=4)
        {
            return self.append(event);
        }

        let v_end = self.current - last.v_offset;
        let vs = self.at(v_end).vs;
        let pos = v_end - (vs.info().len - 1) + tone_position(vs, v_end == self.current);
        self.set_tone(event, pos, tone)
    }

    fn set_tone(&mut self, event: Event, pos: i32, tone: u8) -> bool {
        let current_tone = self.at(pos).tone;
        if current_tone == 0 && tone == 0 {
            return self.append(event);
        }
        self.mark_change(pos);
        if current_tone == tone {
            self.at_mut(pos).tone = 0;
            return self.append_after_undo(event);
        }
        self.at_mut(pos).tone = tone;
        true
    }

    /// Moves a tone when a mark changed which vowel should carry it.
    fn move_tone(&mut self, from: i32, to: i32, tone: u8) {
        if from != to && tone != 0 {
            self.mark_change(to);
            self.at_mut(to).tone = tone;
            self.mark_change(from);
            self.at_mut(from).tone = 0;
        }
    }

    /// Stores `vs` in the entries of the vowel sequence starting at `v_start`.
    fn set_vowels(&mut self, v_start: i32, vs: Vs) {
        for len in 1..=vs.info().len {
            self.at_mut(v_start + len - 1).vs = vs.prefix(len);
        }
    }

    /// The sequence spelled by the entries at `v_start`, `len` letters long.
    fn vowels_at(&self, v_start: i32, len: i32) -> Vs {
        let mut letters = [Sym::NonVn; 3];
        for (i, letter) in letters.iter_mut().enumerate().take(len as usize) {
            *letter = self.at(v_start + i as i32).sym;
        }
        Vs::lookup(letters)
    }

    /// Replaces the vowel at `pos` with `plain` to undo a mark on it, then
    /// appends the key that asked for it.
    fn remove_mark(&mut self, event: Event, v_start: i32, pos: i32, plain: Sym, vs: Vs) -> bool {
        let v_end = self.current - self.at(self.current).v_offset;
        let cur_tone_pos = v_start + tone_position(vs, v_end == self.current);
        let tone = self.at(cur_tone_pos).tone;

        self.mark_change(pos);
        self.at_mut(pos).sym = plain;

        let new_vs = self.vowels_at(v_start, vs.info().len);
        self.set_vowels(v_start, new_vs);
        let new_tone_pos = v_start + tone_position(new_vs, v_end == self.current);
        self.move_tone(cur_tone_pos, new_tone_pos, tone);
        self.append_after_undo(event)
    }

    fn apply_roof(&mut self, event: Event) -> bool {
        if self.current < 0 || self.at(self.current).v_offset < 0 {
            return self.append(event);
        }
        let target = match event.action {
            Action::Roof(sym) => Some(sym),
            _ => None,
        };

        let v_end = self.current - self.at(self.current).v_offset;
        let vs = self.at(v_end).vs;
        let info = vs.info();
        let v_start = v_end - (info.len - 1);
        let cur_tone_pos = v_start + tone_position(vs, v_end == self.current);
        let tone = self.at(cur_tone_pos).tone;

        // ươ takes the roof as a whole, becoming uô.
        let both_letters = matches!(vs, Vs::UwO | Vs::UwOw | Vs::UwOI | Vs::UwOwI);
        let new_vs = if both_letters {
            Vs::lookup([Sym::U, Sym::Oo, info.letters[2]])
        } else {
            info.with_roof
        };

        if new_vs == Vs::Nil {
            if info.roof_pos == -1 {
                return self.append(event);
            }
            let pos = v_start + info.roof_pos;
            let marked = self.at(pos).sym;
            if target.is_some_and(|t| t != marked) {
                return self.append(event);
            }
            let plain = match marked {
                Sym::Aa => Sym::A,
                Sym::Ee => Sym::E,
                _ => Sym::O,
            };
            return self.remove_mark(event, v_start, pos, plain, vs);
        }

        let new_info = new_vs.info();
        let marked = new_info.letters[new_info.roof_pos as usize];
        if target.is_some_and(|t| t != marked)
            || !is_valid_cvc(self.initial_of(self.current), new_vs, self.final_of(self.current))
        {
            return self.append(event);
        }

        if both_letters {
            self.mark_change(v_start);
            self.at_mut(v_start).sym = Sym::U;
            self.at_mut(v_start + 1).sym = Sym::Oo;
        } else {
            let pos = v_start + new_info.roof_pos;
            self.mark_change(pos);
            self.at_mut(pos).sym = marked;
        }
        self.set_vowels(v_start, new_vs);

        let new_tone_pos = v_start + tone_position(new_vs, v_end == self.current);
        self.move_tone(cur_tone_pos, new_tone_pos, tone);
        true
    }

    fn apply_hook(&mut self, event: Event) -> bool {
        if self.current < 0 || self.at(self.current).v_offset < 0 {
            return self.append(event);
        }

        let v_end = self.current - self.at(self.current).v_offset;
        let vs = self.at(v_end).vs;
        let info = vs.info();
        let [v0, v1, _] = info.letters;

        if info.len > 1
            && event.action != Action::Bowl
            && matches!(v0, Sym::U | Sym::Uw)
            && matches!(v1, Sym::O | Sym::Ow | Sym::Oo)
        {
            return self.apply_hook_to_uo(event);
        }

        let v_start = v_end - (info.len - 1);
        let cur_tone_pos = v_start + tone_position(vs, v_end == self.current);
        let tone = self.at(cur_tone_pos).tone;

        // Whether this key's mark can sit on `sym`.
        let applies = |sym: Sym| match event.action {
            Action::Bowl => sym == Sym::Aw,
            Action::HookUo => sym != Sym::Aw,
            _ => true,
        };

        if info.with_hook == Vs::Nil {
            if info.hook_pos == -1 {
                return self.append(event);
            }
            let pos = v_start + info.hook_pos;
            let marked = self.at(pos).sym;
            if !applies(marked) {
                return self.append(event);
            }
            let plain = match marked {
                Sym::Aw => Sym::A,
                Sym::Uw => Sym::U,
                _ => Sym::O,
            };
            return self.remove_mark(event, v_start, pos, plain, vs);
        }

        let new_vs = info.with_hook;
        let new_info = new_vs.info();
        let marked = new_info.letters[new_info.hook_pos as usize];
        if !applies(marked)
            || !is_valid_cvc(self.initial_of(self.current), new_vs, self.final_of(self.current))
        {
            return self.append(event);
        }

        let pos = v_start + new_info.hook_pos;
        self.mark_change(pos);
        self.at_mut(pos).sym = marked;
        self.set_vowels(v_start, new_vs);

        let new_tone_pos = v_start + tone_position(new_vs, v_end == self.current);
        self.move_tone(cur_tone_pos, new_tone_pos, tone);
        true
    }

    /// Hooks on u+o sequences, which mark both letters at once: uo → ươ.
    fn apply_hook_to_uo(&mut self, event: Event) -> bool {
        let last = *self.at(self.current);
        let v_end = self.current - last.v_offset;
        let vs = self.at(v_end).vs;
        let info = vs.info();
        let v_start = v_end - (info.len - 1);
        let [v0, v1, v2] = info.letters;
        let cur_tone_pos = v_start + tone_position(vs, v_end == self.current);
        let tone = self.at(cur_tone_pos).tone;

        let mut hook_removed = false;
        let new_vs;
        if v0 == Sym::U {
            if matches!(v1, Sym::O | Sym::Oo) {
                // "thuơ": after th, only the o takes the horn.
                if matches!(vs, Vs::UO | Vs::UOo)
                    && v_end == self.current
                    && last.form == Form::Cv
                    && self.at(self.current - 2).cs == Cs::Th
                {
                    new_vs = Vs::UOw;
                    self.mark_change(v_start + 1);
                    self.at_mut(v_start + 1).sym = Sym::Ow;
                } else {
                    new_vs = info.with_hook.info().with_hook;
                    self.mark_change(v_start);
                    self.at_mut(v_start).sym = Sym::Uw;
                    self.at_mut(v_start + 1).sym = Sym::Ow;
                }
            } else {
                new_vs = info.with_hook;
                self.mark_change(v_start);
                self.at_mut(v_start).sym = Sym::Uw;
            }
        } else if v1 == Sym::O {
            new_vs = info.with_hook;
            self.mark_change(v_start + 1);
            self.at_mut(v_start + 1).sym = Sym::Ow;
        } else {
            new_vs = Vs::lookup([Sym::U, Sym::O, v2]);
            self.mark_change(v_start);
            self.at_mut(v_start).sym = Sym::U;
            self.at_mut(v_start + 1).sym = Sym::O;
            hook_removed = true;
        }

        self.set_vowels(v_start, new_vs);
        let new_tone_pos = v_start + tone_position(new_vs, v_end == self.current);
        self.move_tone(cur_tone_pos, new_tone_pos, tone);

        if hook_removed {
            self.append_after_undo(event);
        }
        true
    }

    fn apply_dd(&mut self, event: Event) -> bool {
        if self.current < 0 {
            return self.append(event);
        }

        // Allow đ in abbreviations that are not Vietnamese words.
        let last = *self.at(self.current);
        if last.form == Form::Foreign
            && last.sym == Sym::D
            && (self.current == 0 || !self.at(self.current - 1).sym.is_vowel())
        {
            self.single_mode = true;
            self.mark_change(self.current);
            let entry = self.at_mut(self.current);
            entry.cs = Cs::Dd;
            entry.sym = Sym::Dd;
            self.set_shape(self.current, Form::C, 0, -1, -1);
            return true;
        }

        if last.c1_offset < 0 {
            return self.append(event);
        }

        let pos = self.current - last.c1_offset;
        match self.at(pos).cs {
            Cs::D => {
                self.mark_change(pos);
                let entry = self.at_mut(pos);
                entry.cs = Cs::Dd;
                entry.sym = Sym::Dd;
                true
            }
            Cs::Dd => {
                self.mark_change(pos);
                let entry = self.at_mut(pos);
                entry.cs = Cs::D;
                entry.sym = Sym::D;
                self.append_after_undo(event)
            }
            _ => self.append(event),
        }
    }

    /// Telex `[` and `]`: ơ and ư where they fit, else the literal bracket.
    fn apply_map(&mut self, event: Event) -> bool {
        self.append(event);
        if self.current < 0 {
            return false;
        }
        if !matches!(self.at(self.current).form, Form::Empty | Form::Foreign) {
            return true;
        }

        // The letter does not fit here. Undo it and type the key literally,
        // also removing the letter before it if this key produced that one.
        self.current -= 1;
        let mut undone = false;
        if self.current >= 0 {
            let entry = *self.at(self.current);
            if !matches!(entry.form, Form::Empty | Form::Foreign)
                && entry.sym == event.sym
                && entry.caps == event.caps
            {
                self.mark_change(self.current);
                self.current -= 1;
                undone = true;
            }
        }

        let literal = Event {
            action: Action::Plain,
            class: char_class(event.key),
            sym: Sym::NonVn,
            caps: false,
            ..event
        };
        let appended = self.append(literal);
        if undone {
            self.single_mode = false;
        }
        appended || undone
    }

    fn apply_telex_w(&mut self, event: Event) -> bool {
        // "ww" turns a standalone ư back into a plain w.
        if self.current >= 0 {
            let last = *self.at(self.current);
            if last.sym == Sym::Uw && last.form == Form::V && matches!(last.key, b'w' | b'W') {
                self.mark_change(self.current);
                let entry = self.at_mut(self.current);
                entry.form = Form::Foreign;
                entry.sym = Sym::NonVn;
                entry.key = if last.caps { b'W' } else { b'w' };
                entry.v_offset = -1;
                self.single_mode = false;
                return true;
            }
            if last.v_offset >= 0 {
                return self.apply_hook(Event { action: Action::HookAny, ..event });
            }
            // Mid-word in something foreign, as in "windows": just a w.
            if last.form == Form::Foreign {
                return self.append(event);
            }
        }

        self.current += 1;
        let cur = self.current;
        let entry = self.at_mut(cur);
        entry.sym = Sym::Uw;
        entry.caps = event.caps;
        entry.tone = 0;
        entry.key = event.key;
        entry.vs = Vs::Uw;
        self.set_shape(cur, Form::V, -1, -1, 0);
        self.mark_change(cur);
        true
    }
}

/// Which vowel of `vs` carries the tone. `terminated` means nothing follows the
/// vowels, which decides between "hóa"-style positions in open syllables.
fn tone_position(vs: Vs, terminated: bool) -> i32 {
    let info = vs.info();
    if info.len == 1 {
        0
    } else if info.roof_pos != -1 {
        info.roof_pos
    } else if info.hook_pos != -1 {
        if matches!(vs, Vs::UwOw | Vs::UwOwI | Vs::UwOwU) { 1 } else { info.hook_pos }
    } else if info.len == 3 || matches!(vs, Vs::OA | Vs::OE | Vs::UY) {
        1
    } else if terminated {
        0
    } else {
        1
    }
}
