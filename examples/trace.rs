//! Prints what appears on screen after each key of a line of typing, for
//! `tools/make_demo.py`. Usage: trace <telex|vni> <keys>

use minikey::{Engine, Method};

fn main() {
    let mut args = std::env::args().skip(1);
    let method = if args.next().as_deref() == Some("vni") { Method::Vni } else { Method::Telex };
    let keys = args.next().expect("usage: trace <telex|vni> <keys>");

    let mut engine = Engine::new(method);
    let mut screen: Vec<u16> = Vec::new();
    for key in keys.bytes() {
        if key == b' ' {
            engine.end_word();
            screen.push(u16::from(key));
        } else if let Some(edit) = engine.key(key) {
            screen.truncate(screen.len() - edit.backspaces);
            screen.extend_from_slice(edit.text());
        } else {
            screen.push(u16::from(key));
        }
        println!("{}\t{}", key as char, String::from_utf16_lossy(&screen));
    }
}
