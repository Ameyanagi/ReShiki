use super::*;
use crate::chemistry::cdxml::{bonds, parse, presentation, tree::Tree};

fn nested(levels: usize) -> String {
    format!(
        "<CDXML>{}{}</CDXML>",
        "<group>".repeat(levels),
        "</group>".repeat(levels)
    )
}

/// A file every reader accepts, with `count` attributes on one element.
fn attributes(count: usize) -> String {
    let attributes: String = (0..count).map(|i| format!(" a{i}='1'")).collect();
    format!(
        "<CDXML BondLength=\"17\" BondSpacing=\"18\" LineWidth=\"0.75\" BoldWidth=\"2.6\" \
        MarginWidth=\"2\" HashSpacing=\"2.6\" LabelFont=\"3\" LabelSize=\"12\">\
        <fonttable><font id=\"3\" name=\"Arial\"/></fonttable><page{attributes}/></CDXML>"
    )
}

/// Every reader that hands CDXML or stationery text to the XML parser, and
/// its error.
fn readers(text: &str) -> [(&'static str, Result<(), String>); 5] {
    fn error<T, E: ToString>(result: Result<T, E>) -> Result<(), String> {
        result.map(|_| ()).map_err(|e| e.to_string())
    }
    [
        ("tree", error(Tree::parse_import(text))),
        ("molecules", error(parse::read(text))),
        ("bonds", error(bonds::read_native(text, &[], 1., &[]))),
        ("presentation", error(presentation::parse(text))),
        (
            "style",
            error(crate::document_styles::load_bytes(
                std::path::Path::new("style.cdxml"),
                text.as_bytes().to_vec(),
            )),
        ),
    ]
}

#[test]
fn deep_nesting_is_refused_before_the_parser_can_overflow_its_stack() {
    // 3,000 levels overflow the parser on the 2 MiB stack of a spawned thread.
    let refused = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| readers(&nested(3_000)))
        .unwrap()
        .join()
        .unwrap();
    for (reader, result) in refused {
        assert!(result.is_err(), "{reader}");
    }
}

#[test]
fn nesting_limit_keeps_the_depth_the_readers_accepted_before() {
    assert_eq!(check(&nested(64)), Ok(()));
    assert!(Tree::parse_import(&nested(64)).is_ok());
    assert!(presentation::parse(&nested(64)).is_ok());
    assert_eq!(check(&nested(65)), Err(Refused::Depth));
    // Siblings, empty elements, comments, text and CDATA do not add depth.
    let flat = format!(
        "<?xml version='1.0'?><!DOCTYPE CDXML SYSTEM 'x.dtd'><CDXML>{}</CDXML>",
        "<a><b/><!-- <c> --><![CDATA[<d>]]>x &gt; y</a>".repeat(200)
    );
    assert_eq!(check(&flat), Ok(()));
}

#[test]
fn an_element_with_too_many_attributes_is_refused_before_parsing() {
    assert_eq!(check(&attributes(256)), Ok(()));
    assert_eq!(check(&attributes(257)), Err(Refused::Attributes));
    // The count is per element, and quotes in text are not attributes.
    let many = format!(
        "<CDXML>{}</CDXML>",
        "<n a='1' b=\"2\">it's \"text\"</n>".repeat(300)
    );
    assert_eq!(check(&many), Ok(()));
    for (reader, result) in readers(&attributes(256)) {
        assert_eq!(result, Ok(()), "{reader}");
    }
    for (reader, result) in readers(&attributes(257)) {
        let error = result.unwrap_err();
        assert!(
            error.ends_with("more than 256 attributes"),
            "{reader}: {error}"
        );
    }
}

#[test]
fn entity_declarations_are_refused_by_every_reader() {
    let text = "<!DOCTYPE CDXML [<!ENTITY font \"Arial\">]>\
        <CDXML BondLength=\"17\" BondSpacing=\"18\" LineWidth=\"0.75\" BoldWidth=\"2.6\" \
        MarginWidth=\"2\" HashSpacing=\"2.6\" LabelFont=\"3\" LabelSize=\"12\">\
        <fonttable><font id=\"3\" name=\"&font;\"/></fonttable></CDXML>";
    assert_eq!(check(text), Err(Refused::Dtd));
    for (reader, result) in readers(text) {
        assert!(result.is_err(), "{reader}");
    }
    let error = crate::document_styles::load_bytes(
        std::path::Path::new("style.cdxml"),
        text.as_bytes().to_vec(),
    )
    .unwrap_err();
    assert_eq!(
        error,
        "Invalid ChemDraw style: DTD declarations are not supported"
    );
}

fn expanded(text: &str) -> Option<String> {
    roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 1_000,
        },
    )
    .ok()?
    .root_element()
    .text()
    .map(str::to_owned)
}

#[test]
fn no_declaration_hides_an_entity_or_nesting_from_the_scan() {
    // The parser ends these declarations at the first `>`, whatever they
    // quote or open, so the entity after each one is live.
    for hidden in [
        "<!ELEMENT x ' ><!ENTITY a \"v\">]><x>&a;</x>",
        "<!ATTLIST x y CDATA ' ><!ENTITY a \"v\">]><x>&a;</x>",
        "<!NOTATION n SYSTEM ' ><!ENTITY a \"v\">]><x>&a;</x>",
        "<!ELEMENT x (a|b) '><!ENTITY a \"v\">]><x>&a;</x><!-- ' -->",
        "<!ELEMENT x <!-- ><!ENTITY a \"v\"><!-- -->]><x>&a;</x>",
        "<!-- ' --><?p \" ?> <!ENTITY a \"v\">]><x>&a;</x>",
    ] {
        let text = format!("<!DOCTYPE x [{hidden}");
        assert_eq!(expanded(&text).as_deref(), Some("v"), "{text}");
        assert_eq!(check(&text), Err(Refused::Dtd), "{text}");
    }
    // A declaration that opens a comment must not hide the nesting after it.
    let text = format!("<!DOCTYPE x [<!ELEMENT x <!-- >]>{}<!-- -->", nested(3_000));
    assert_eq!(check(&text), Err(Refused::Dtd));
}

#[test]
fn an_external_dtd_and_an_inert_internal_subset_are_accepted() {
    for text in [
        "<?xml version=\"1.0\"?><!DOCTYPE CDXML SYSTEM \"http://www.cambridgesoft.com/xml/cdxml.dtd\" ><CDXML/>",
        "<!DOCTYPE CDXML PUBLIC 'a[b' \"c>d\"><CDXML/>",
        "<!DOCTYPE CDXML [ <!-- <!ENTITY a 'v'> --> <?p <!ENTITY a 'v'> ?> ]><CDXML/>",
        "<CDXML><!-- <!ENTITY a 'v'> --><t><![CDATA[<!ENTITY a 'v'>]]></t></CDXML>",
    ] {
        assert_eq!(check(text), Ok(()), "{text}");
        assert!(Tree::parse_import(text).is_ok(), "{text}");
    }
}
