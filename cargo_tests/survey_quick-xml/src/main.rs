use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;
use std::io::Cursor;

// A fixed, self-contained XML document. No I/O, no network, no files.
const DOC: &str = r#"<?xml version="1.0"?>
<catalog count="2">
  <book id="b1" lang="en">
    <title>Rust In Action</title>
    <price>39</price>
  </book>
  <book id="b2" lang="fr">
    <title>Programmer en Rust</title>
    <price>42</price>
  </book>
</catalog>"#;

fn main() {
    // --- Part 1: Reader. Walk the event stream, count structure deterministically. ---
    let mut reader = Reader::from_str(DOC);
    reader.config_mut().trim_text(true);

    let mut elements: u64 = 0; // count of Start events (open tags)
    let mut empty_elements: u64 = 0; // count of Empty (self-closing) events
    let mut end_events: u64 = 0;
    let mut attributes: u64 = 0; // total attributes across all start/empty tags
    let mut text_nodes: u64 = 0;
    let mut text_bytes: u64 = 0; // total decoded text length
    let mut price_sum: u64 = 0; // extract <price> values and sum them
    let mut in_price = false;
    let mut first_title = String::new(); // extracted value: first <title> text
    let mut read_error = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                elements += 1;
                attributes += count_attrs(&e);
                if e.name().as_ref() == b"price" {
                    in_price = true;
                }
            }
            Ok(Event::Empty(e)) => {
                empty_elements += 1;
                attributes += count_attrs(&e);
            }
            Ok(Event::End(_)) => {
                end_events += 1;
                in_price = false;
            }
            Ok(Event::Text(t)) => {
                text_nodes += 1;
                match t.decode() {
                    Ok(cow) => {
                        let s = cow.as_ref();
                        text_bytes += s.len() as u64;
                        if in_price {
                            // Parse the integer price without panicking.
                            if let Ok(v) = s.trim().parse::<u64>() {
                                price_sum += v;
                            }
                        }
                        if first_title.is_empty() && s == "Rust In Action" {
                            first_title.push_str(s);
                        }
                    }
                    Err(_) => {
                        read_error = true;
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {
                // Decl, Comment, CData, PI, DocType — ignored for counting.
            }
            Err(_) => {
                read_error = true;
                break;
            }
        }
    }

    println!("read_error = {}", read_error);
    println!("start_elements = {}", elements);
    println!("empty_elements = {}", empty_elements);
    println!("end_events = {}", end_events);
    println!("attributes = {}", attributes);
    println!("text_nodes = {}", text_nodes);
    println!("text_bytes = {}", text_bytes);
    println!("price_sum = {}", price_sum);
    println!("first_title = {}", first_title);

    // --- Part 2: Writer. Build a small document and measure its byte length. ---
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut write_error = false;

    // <note priority="high"><to>world</to></note>
    let mut note = BytesStart::new("note");
    note.push_attribute(("priority", "high"));
    if writer.write_event(Event::Start(note)).is_err() {
        write_error = true;
    }

    if writer
        .write_event(Event::Start(BytesStart::new("to")))
        .is_err()
    {
        write_error = true;
    }
    if writer
        .write_event(Event::Text(BytesText::new("world")))
        .is_err()
    {
        write_error = true;
    }
    if writer
        .write_event(Event::End(BytesEnd::new("to")))
        .is_err()
    {
        write_error = true;
    }
    if writer
        .write_event(Event::End(BytesEnd::new("note")))
        .is_err()
    {
        write_error = true;
    }

    let out = writer.into_inner().into_inner();
    println!("write_error = {}", write_error);
    println!("written_len = {}", out.len());

    // Deterministic check of the written content rather than echoing raw bytes.
    let expected = br#"<note priority="high"><to>world</to></note>"#;
    println!("written_matches = {}", out.as_slice() == expected);

    // --- Part 3: Round-trip the written doc back through the Reader. ---
    match core::str::from_utf8(out.as_slice()) {
        Ok(s) => {
            let mut rt = Reader::from_str(s);
            let mut rt_starts: u64 = 0;
            let mut rt_err = false;
            loop {
                match rt.read_event() {
                    Ok(Event::Start(_)) => rt_starts += 1,
                    Ok(Event::Eof) => break,
                    Ok(_) => {}
                    Err(_) => {
                        rt_err = true;
                        break;
                    }
                }
            }
            println!("roundtrip_starts = {}", rt_starts);
            println!("roundtrip_error = {}", rt_err);
        }
        Err(_) => {
            println!("roundtrip_starts = <non-utf8>");
            println!("roundtrip_error = true");
        }
    }

    println!("== survey_quick-xml done ==");
}

// Count attributes on a start/empty tag, skipping any malformed ones (no panic).
fn count_attrs(e: &BytesStart) -> u64 {
    let mut n: u64 = 0;
    for attr in e.attributes() {
        if attr.is_ok() {
            n += 1;
        }
    }
    n
}
