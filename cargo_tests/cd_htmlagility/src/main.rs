//! Real proof that Rust can drive `HtmlAgilityPack` (a third-party NuGet package, DOM-tree-shaped
//! API) via `cargo dotnet add-nuget` bindings: parse a small HTML fragment and query it with
//! XPath + DOM traversal (`SelectSingleNode`, `InnerText`, `GetAttributeValue`, `FirstChild` /
//! `NextSibling`).
//!
//! NOTE: `HtmlNodeCollection`'s `Item` indexer was reflected with the WRONG overload — spinacz's
//! reflection picked `get_Item(HtmlNode) -> int` instead of the real DOM indexer
//! `get_Item(int) -> HtmlNode` (C# has both overloads named `Item`/`this[]`, and the generator
//! keeps only the last one it saw). This test deliberately avoids `HtmlNodeCollection::get_item`
//! and uses `SelectSingleNode` + manual `FirstChild`/`NextSibling` walking instead, which are
//! unaffected and give full coverage of the DOM-tree shape.
#![allow(dead_code)]

mod nuget;

use nuget::htmlagilitypack::HtmlAgilityPack::{
    HtmlDocument, HtmlDocument_Methods, HtmlNode, HtmlNode_Methods, HtmlNodeCollection_Methods,
};
use nuget::htmlagilitypack::UpcastTo;
use mycorrhiza::system::console::Console;
use mycorrhiza::system::DotNetString;
use mycorrhiza::System;
use mycorrhiza::System::Object;

// `HtmlNode` does not override `==`/`op_Equality` (unlike some managed types this backend's
// `.is_null()` helper assumes), so `MissingMethodException` results from calling it directly.
// Reference-null-check via `System.Object.ReferenceEquals` (upcast) instead — always safe.
fn node_is_null(n: HtmlNode) -> bool {
    let obj: Object = UpcastTo::<System::Object>::upcast(n);
    System::Object::reference_equals(obj, System::Object::null())
}

fn main() -> std::process::ExitCode {
    let mut pass: u32 = 0;
    let mut total: u32 = 0;
    macro_rules! chk {
        ($got:expr, $want:expr) => {{
            total += 1;
            if $got == $want {
                pass += 1;
            } else {
                Console::writeln_u64(900_000_000 + total as u64);
            }
        }};
    }

    let html = r#"<html><body>
        <ul id="fruits">
            <li class="item">Apple</li>
            <li class="item">Banana</li>
            <li class="item">Cherry</li>
        </ul>
        <a href="https://example.com/page">Click here</a>
    </body></html>"#;

    let doc: HtmlDocument = HtmlDocument::new();
    let html_str: DotNetString = html.into();
    doc.load_html(html_str.handle());

    let root: HtmlNode = doc.get_document_node();

    // XPath query for the <a> tag's href attribute.
    let a_xpath: DotNetString = "//a".into();
    let a_node = root.select_single_node(a_xpath.handle());
    let has_a = !node_is_null(a_node);
    chk!(has_a, true);
    let href_name: DotNetString = "href".into();
    let default_val: DotNetString = "".into();
    let href = String::from(DotNetString::from_handle(
        a_node.get_attribute_value(href_name.handle(), default_val.handle()),
    ));
    chk!(href.as_str(), "https://example.com/page");
    let link_text = String::from(DotNetString::from_handle(a_node.get_inner_text()));
    chk!(link_text.as_str(), "Click here");

    // XPath query for the <ul id="fruits"> element, then walk its <li> children manually
    // (exercising FirstChild/NextSibling DOM traversal, not the broken collection indexer).
    let ul_xpath: DotNetString = "//ul[@id='fruits']".into();
    let ul_node = root.select_single_node(ul_xpath.handle());
    let has_ul = !node_is_null(ul_node);
    chk!(has_ul, true);

    let li_xpath: DotNetString = "li".into();
    let items = ul_node.select_nodes(li_xpath.handle());
    let item_count = items.get_count();
    chk!(item_count, 3);

    // Walk the <ul>'s element children (skipping whitespace text nodes) via FirstChild/NextSibling
    // and collect the <li> inner text, proving real DOM-tree navigation (not just XPath).
    let mut fruit_names: Vec<String> = Vec::new();
    let li_tag: DotNetString = "li".into();
    let mut cur = ul_node.get_first_child();
    loop {
        if node_is_null(cur) {
            break;
        }
        let name = String::from(DotNetString::from_handle(cur.get_name()));
        if name == "li" {
            let text = String::from(DotNetString::from_handle(cur.get_inner_text()));
            fruit_names.push(text);
        }
        cur = cur.get_next_sibling();
    }
    let _ = li_tag;

    chk!(fruit_names.len(), 3);
    if fruit_names.len() == 3 {
        chk!(fruit_names[0].as_str(), "Apple");
        chk!(fruit_names[1].as_str(), "Banana");
        chk!(fruit_names[2].as_str(), "Cherry");
    } else {
        total += 3;
    }

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        println!("== cd_htmlagility done ==");
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
