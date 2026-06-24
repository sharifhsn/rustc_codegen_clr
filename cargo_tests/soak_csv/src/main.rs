use csv::{ReaderBuilder, StringRecord, Writer, WriterBuilder};

// In-memory CSV: header + 4 data rows. No file IO anywhere — we read from a
// &[u8] and write into a Vec<u8>/String. The "amount" column is the numeric
// column we sum; integer values keep the output exact (no float-repr drift).
const CSV_INPUT: &str = "\
name,amount,active
alice,100,true
bob,250,false
carol,75,true
dave,330,true
";

fn main() {
    // --- Parse phase: ReaderBuilder over an in-memory byte slice. ---
    let mut rdr = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(CSV_INPUT.as_bytes());

    // Header introspection (deterministic; comes straight from the input).
    match rdr.headers() {
        Ok(headers) => {
            let joined: Vec<&str> = headers.iter().collect();
            println!("header = {}", joined.join("|"));
            println!("num_columns = {}", headers.len());
        }
        Err(_) => {
            println!("header = <error>");
            println!("num_columns = 0");
        }
    }

    // Accumulate: row count, sum of the "amount" column (index 1), count of
    // rows whose "active" column (index 2) is "true". All integer arithmetic.
    let mut row_count: u64 = 0;
    let mut amount_sum: i64 = 0;
    let mut active_count: u64 = 0;
    let mut parse_errors: u64 = 0;
    // Keep the parsed records so we can write them back out below.
    let mut records: Vec<StringRecord> = Vec::new();

    for result in rdr.records() {
        match result {
            Ok(record) => {
                row_count += 1;
                // Column 1 = amount. Parse without panicking.
                match record.get(1) {
                    Some(field) => match field.trim().parse::<i64>() {
                        Ok(v) => amount_sum += v,
                        Err(_) => parse_errors += 1,
                    },
                    None => parse_errors += 1,
                }
                // Column 2 = active flag.
                if let Some(flag) = record.get(2) {
                    if flag.trim() == "true" {
                        active_count += 1;
                    }
                }
                records.push(record);
            }
            Err(_) => {
                parse_errors += 1;
            }
        }
    }

    println!("row_count = {}", row_count);
    println!("amount_sum = {}", amount_sum);
    println!("active_count = {}", active_count);
    println!("parse_errors = {}", parse_errors);

    // --- Write phase: serialize the records back to a CSV String. ---
    // WriterBuilder into an in-memory Vec<u8> (no file IO). We re-emit the
    // header plus every parsed record, then recover it as a String.
    let mut wtr: Writer<Vec<u8>> = WriterBuilder::new().from_writer(Vec::new());

    let mut write_ok = true;
    if wtr.write_record(&["name", "amount", "active"]).is_err() {
        write_ok = false;
    }
    for rec in &records {
        if wtr.write_record(rec).is_err() {
            write_ok = false;
        }
    }

    // Flush + recover the underlying buffer without unwrap/expect.
    let output = match wtr.into_inner() {
        Ok(buf) => match String::from_utf8(buf) {
            Ok(s) => s,
            Err(_) => {
                write_ok = false;
                String::from("<non-utf8>")
            }
        },
        Err(_) => {
            write_ok = false;
            String::from("<flush-error>")
        }
    };

    println!("write_ok = {}", write_ok);
    println!("output_len = {}", output.len());
    // Round-trip check: the re-emitted CSV should equal the original input.
    println!("roundtrip_matches = {}", output == CSV_INPUT);

    // Emit the produced CSV verbatim so the bytes are part of the diff.
    // (Trailing newline already present from the writer; avoid double blank.)
    print!("--- output begin ---\n{}--- output end ---\n", output);

    println!("== soak_csv done ==");
}
